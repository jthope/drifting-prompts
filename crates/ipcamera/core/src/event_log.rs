//! The events the cameras saw: `data\events\events.json`.
//!
//! What an event saved is a snapshot in the snapshots folder and recordings in the videos folder,
//! where the pages list them with the rest, and the event names them ([`EventFile`],
//! [`EventLog::link`]). The log keeps no files of its own. The camera's watch knows what it saw
//! but not what was saved, so [`EventLog::record`] keeps an event's files as what it saw changes.
//!
//! The log keeps the newest [`MOST_EVENTS`]: older ones go as new ones come, and leave their
//! files, which are the snapshots' and videos' like any others (and under their limits). Deleting
//! an event on purpose ([`EventLog::delete`], [`EventLog::clear`]) returns it, so the program can
//! delete its files with it.
//!
//! It is saved as the settings are, on [`FileStore`]: the whole file at each change, written then
//! renamed, never over a file it could not read, and read again first when another program wrote
//! it since. A change is an event starting, a sighting starting or ending, what it saw growing or a
//! file linked, a few a minute at most, so writing the whole file each time costs little.
//!
//! The file lists events oldest first, as they came; [`EventLog::events`] gives them newest first,
//! as the app lists them.

use std::{collections::HashMap, path::Path};

use dp_store::{Document, FileStore, Json};
use serde::{Deserialize, Serialize};

use crate::{CameraEvent, CameraId, StoreError};

/// How many events the log keeps.
pub const MOST_EVENTS: usize = 1000;

/// `events.json` as it is on disk.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LogFile {
    version: u32,
    events: Vec<CameraEvent>,
}

impl Document for LogFile {
    type Format = Json;
    const VERSION: u32 = 1;

    fn version_mut(&mut self) -> Option<&mut u32> {
        Some(&mut self.version)
    }
}

/// A file an event saved, by its name, to link to the event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventFile {
    /// Its snapshot, in the snapshots folder.
    Snapshot(String),
    /// One of its recordings, in the videos folder.
    Video(String),
}

/// The event log, and the file it is saved in.
pub struct EventLog {
    file: FileStore<LogFile>,
}

impl EventLog {
    /// The log's folder inside `data`.
    pub const FOLDER: &'static str = "events";
    /// The log's file inside its folder.
    pub const FILE: &'static str = "events.json";

    /// Opens the log in `folder` (`data\events`). A missing file is an empty log, and nothing is
    /// written until the first event.
    pub fn open(folder: &Path) -> Self {
        Self {
            file: FileStore::open(folder.join(Self::FILE)),
        }
    }

    /// An empty log kept in memory only.
    pub fn in_memory() -> Self {
        Self {
            file: FileStore::in_memory(),
        }
    }

    /// What went wrong opening the file, for the app to tell the user.
    pub fn problem(&self) -> Option<&str> {
        self.file.problem()
    }

    /// Goes up by one at every change.
    pub fn revision(&self) -> u64 {
        self.file.revision()
    }

    /// Every event, newest first.
    pub fn events(&self) -> Vec<CameraEvent> {
        self.file
            .read(|file| file.events.iter().rev().cloned().collect())
    }

    /// Adds `event`, or puts what it saw in the place of the one with its id, keeping the files
    /// linked to that one: the watch that saw it never knows them. The oldest events go to make
    /// room, and leave their files.
    pub fn record(&self, event: CameraEvent) -> Result<(), StoreError> {
        self.file.update(|file| {
            match file.events.iter_mut().find(|held| held.id == event.id) {
                Some(held) => {
                    let snapshot = held.snapshot.take();
                    let videos = std::mem::take(&mut held.videos);
                    *held = CameraEvent {
                        snapshot,
                        videos,
                        ..event
                    };
                }
                None => file.events.push(event),
            }
            let over = file.events.len().saturating_sub(MOST_EVENTS);
            file.events.drain(..over);
            Ok(())
        })
    }

    /// Links `saved` to event `id`: its snapshot, or one more of its recordings. Linking one it
    /// already has, or to an event the log no longer has, changes nothing and writes nothing.
    pub fn link(&self, id: &str, saved: EventFile) -> Result<(), StoreError> {
        self.file.update(|file| {
            let Some(event) = file.events.iter_mut().find(|event| event.id == id) else {
                return Ok(());
            };
            match saved {
                EventFile::Snapshot(name) => event.snapshot = Some(name),
                EventFile::Video(name) => {
                    if !event.videos.contains(&name) {
                        event.videos.push(name);
                    }
                }
            }
            Ok(())
        })
    }

    /// Puts recording `new` in the place of `old` among event `id`'s files: the same video,
    /// renamed as it closed to carry the tags of everything its camera saw while it recorded.
    /// Changes nothing when the log no longer has the event or the event that file.
    pub fn rename_video(&self, id: &str, old: &str, new: &str) -> Result<(), StoreError> {
        self.file.update(|file| {
            let held = file
                .events
                .iter_mut()
                .find(|event| event.id == id)
                .and_then(|event| event.videos.iter_mut().find(|name| *name == old));
            if let Some(name) = held {
                *name = new.to_string();
            }
            Ok(())
        })
    }

    /// Every file the events saved, by name, with the id of the event that saved it: for the
    /// snapshots and videos pages' Show event.
    pub fn saved_files(&self) -> HashMap<String, String> {
        self.file.read(|file| {
            file.events
                .iter()
                .flat_map(|event| {
                    event
                        .files()
                        .map(|name| (name.to_string(), event.id.clone()))
                })
                .collect()
        })
    }

    /// Ends each event going on that `still_seen` says no camera watch is still seeing, at `at`
    /// (milliseconds since 1970), or at its start for one left over from before the program
    /// started, whose end it can't know.
    pub fn end_unseen(
        &self,
        still_seen: impl Fn(&CameraId) -> bool,
        at: Option<u64>,
    ) -> Result<(), StoreError> {
        self.file.update(|file| {
            for event in &mut file.events {
                if event.ongoing() && !still_seen(&event.camera) {
                    event.ended = Some(at.unwrap_or(event.started).max(event.started));
                }
            }
            Ok(())
        })
    }

    /// Whether the log has event `id`.
    pub fn has(&self, id: &str) -> bool {
        self.file
            .read(|file| file.events.iter().any(|event| event.id == id))
    }

    /// Deletes each event of `ids` that has ended, in one save, returning those deleted. One
    /// going on stays, as [`EventLog::clear`] keeps it; an id the log doesn't have is passed over.
    pub fn delete(&self, ids: &[String]) -> Result<Vec<CameraEvent>, StoreError> {
        self.file.update(|file| {
            let (gone, kept) = std::mem::take(&mut file.events)
                .into_iter()
                .partition(|event| !event.ongoing() && ids.contains(&event.id));
            file.events = kept;
            Ok(gone)
        })
    }

    /// Deletes every event that has ended, returning them. One going on stays: its camera's watch
    /// would only log it again at its next change.
    pub fn clear(&self) -> Result<Vec<CameraEvent>, StoreError> {
        self.file.update(|file| {
            let (ongoing, ended) = std::mem::take(&mut file.events)
                .into_iter()
                .partition(CameraEvent::ongoing);
            file.events = ongoing;
            Ok(ended)
        })
    }

    /// Event `id`, when the log has it.
    pub fn event(&self, id: &str) -> Option<CameraEvent> {
        self.file
            .read(|file| file.events.iter().find(|event| event.id == id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::Detection;

    fn event(started: u64, camera: &str) -> CameraEvent {
        CameraEvent {
            id: format!("{started}-{camera}"),
            camera: CameraId::from(camera),
            started,
            ended: None,
            motion: false,
            detected: vec![Detection::Person],
            sightings: 1,
            snapshot: None,
            videos: Vec::new(),
        }
    }

    #[test]
    fn an_event_is_added_then_replaced_as_it_changes() {
        let folder = TempDir::new().unwrap();
        let log = EventLog::open(folder.path());
        assert!(log.events().is_empty());

        log.record(event(1, "a")).unwrap();
        log.record(event(2, "b")).unwrap();
        let mut ended = event(1, "a");
        ended.ended = Some(5);
        log.record(ended.clone()).unwrap();

        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "2-b", "newest first");
        assert_eq!(events[1], ended);

        let again = EventLog::open(folder.path());
        assert_eq!(again.events(), events, "it is saved");
    }

    #[test]
    fn a_video_renamed_as_it_closes_stays_linked_in_its_place() {
        let log = EventLog::in_memory();
        log.record(event(1, "a")).unwrap();
        log.link("1-a", EventFile::Video("video-1-a.animal.mp4".into()))
            .unwrap();
        log.link("1-a", EventFile::Video("video-9-a.animal.mp4".into()))
            .unwrap();
        log.rename_video(
            "1-a",
            "video-1-a.animal.mp4",
            "video-1-a.vehicle.animal.mp4",
        )
        .unwrap();
        assert_eq!(
            log.event("1-a").unwrap().videos,
            ["video-1-a.vehicle.animal.mp4", "video-9-a.animal.mp4"]
        );
        // Another event's file, or one the event never had, changes nothing.
        log.rename_video("2-b", "video-9-a.animal.mp4", "x.mp4")
            .unwrap();
        log.rename_video("1-a", "gone.mp4", "x.mp4").unwrap();
        assert_eq!(log.event("1-a").unwrap().videos.len(), 2);
    }

    #[test]
    fn the_oldest_go_to_make_room() {
        let log = EventLog::in_memory();
        for started in 0..MOST_EVENTS as u64 {
            log.record(event(started, "a")).unwrap();
        }
        log.record(event(9_999, "a")).unwrap();
        let events = log.events();
        assert_eq!(events.len(), MOST_EVENTS);
        assert_eq!(events.last().unwrap().id, "1-a", "the oldest went");
    }

    #[test]
    fn an_events_files_stay_linked_as_what_it_saw_changes() {
        let folder = TempDir::new().unwrap();
        let log = EventLog::open(folder.path());
        log.record(event(1, "a")).unwrap();
        log.link("1-a", EventFile::Snapshot("snapshot-0-a.person.png".into()))
            .unwrap();
        log.link("1-a", EventFile::Video("video-0-a.person.mp4".into()))
            .unwrap();
        let revision = log.revision();
        log.link("1-a", EventFile::Video("video-0-a.person.mp4".into()))
            .unwrap();
        log.link("9-z", EventFile::Video("video-9-z.mp4".into()))
            .unwrap();
        assert_eq!(
            log.revision(),
            revision,
            "a file it has, or an event it hasn't, writes nothing"
        );

        // The watch sends what it saw, which knows nothing of the files.
        let mut again = event(1, "a");
        again.sightings = 2;
        again.detected.push(Detection::Vehicle);
        log.record(again).unwrap();
        log.link("1-a", EventFile::Video("video-60-a.vehicle.mp4".into()))
            .unwrap();

        let held = EventLog::open(folder.path()).event("1-a").unwrap();
        assert_eq!(held.sightings, 2);
        assert_eq!(held.detected, [Detection::Person, Detection::Vehicle]);
        assert_eq!(held.snapshot.as_deref(), Some("snapshot-0-a.person.png"));
        assert_eq!(
            held.videos,
            ["video-0-a.person.mp4", "video-60-a.vehicle.mp4"]
        );

        let saved = log.saved_files();
        assert_eq!(saved.len(), 3);
        assert_eq!(saved["video-60-a.vehicle.mp4"], "1-a");
    }

    #[test]
    fn events_no_watch_sees_end() {
        let log = EventLog::in_memory();
        log.record(event(10, "a")).unwrap();
        log.record(event(20, "b")).unwrap();

        log.end_unseen(|camera| camera.as_str() == "b", Some(30))
            .unwrap();
        assert!(log.events()[0].ongoing(), "b is still seen");
        assert_eq!(log.events()[1].ended, Some(30), "a is not");

        // One left from before the program started ends where it started.
        log.end_unseen(|_| false, None).unwrap();
        assert_eq!(log.events()[0].ended, Some(20));
    }

    #[test]
    fn events_are_deleted_some_or_all_that_ended() {
        let log = EventLog::in_memory();
        for started in [1, 2, 3, 4] {
            let mut event = event(started, "a");
            event.ended = Some(started + 1);
            log.record(event).unwrap();
        }
        log.record(event(5, "a")).unwrap();
        let ids = |list: &[&str]| list.iter().map(|id| id.to_string()).collect::<Vec<_>>();

        let gone = log.delete(&ids(&["1-a", "3-a", "5-a", "9-a"])).unwrap();
        assert_eq!(
            gone.iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            ["1-a", "3-a"],
            "the one going on and the one the log hasn't stay out of it"
        );
        assert!(log.delete(&ids(&["1-a"])).unwrap().is_empty());
        assert_eq!(log.events().len(), 3);

        assert_eq!(log.clear().unwrap().len(), 2);
        let left = log.events();
        assert_eq!(left.len(), 1, "the one going on stays");
        assert_eq!(log.event("5-a"), Some(left[0].clone()));
    }
}
