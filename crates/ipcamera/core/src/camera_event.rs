//! What a camera saw, as events: when something came into its picture, what it was, how many
//! times it came back, and what the app saved of it.
//!
//! A camera's watch (`camera_watch`) asks it what it sees about once a second, and
//! [`EventTracker`] turns those answers into [`CameraEvent`]s, by the [`EventRules`] it was given.
//! An event starts at the first answer that sees something. Each time the camera sees something
//! is a *sighting*, which ends once the camera has seen nothing for the rules' quiet: at least
//! [`QUIET_FOR_MS`], and as long as an event video's post-roll where events record videos, so a
//! sighting is a video. A person who stands still for a moment, or a detection that drops out for
//! a few answers, stays one sighting. The event stays open for the window the rules give
//! (Settings → Automation's *Group within*, five minutes until changed), and what the camera sees
//! within it is another sighting of the same event. So a car that pulls in, waits and leaves is
//! one event, with one picture and one alert, rather than one for each time the camera lost sight
//! of it. The window runs from the last sighting, so an event lasts for as long as things keep
//! coming back; a window no longer than the quiet groups nothing.
//!
//! What counts is what the camera can tell apart. A camera whose AI tells people, vehicles or
//! animals apart starts an event only when it sees one of them, so plain motion (a tree in the
//! wind, rain, a shadow) logs nothing; a camera that knows only motion starts one on motion. Where
//! the rules let it, plain motion keeps a sighting the AI started going between its detections,
//! for up to [`MOTION_HOLDS_MS`](crate::MOTION_HOLDS_MS) after the AI last saw something, so an
//! animal that grazes in and out of the AI's sight stays one sighting.
//!
//! Each answer that changes the event comes with an [`EventStep`] that says what it did, so the
//! program knows when to take the event's picture (as it starts), when to record (as each
//! sighting starts) and when to stop (as each ends). What the program saves, the event names by
//! file (`snapshot`, `videos`), and only the event log keeps that: the tracker never knows it.
//!
//! The tracker has no clock of its own: each answer comes with the time it was given, so its tests
//! need no waiting. In C# terms it is a small state machine class that a polling loop feeds.

use serde::{Deserialize, Serialize};

use crate::{CameraActivity, CameraId, Detection, MOTION_HOLDS_MS};

/// The least a camera sees nothing for before a sighting ends, in milliseconds: `Now` goes and the
/// sighting's recording stops. The event stays open until its window passes with nothing seen.
pub const QUIET_FOR_MS: u64 = 10_000;

/// How an [`EventTracker`] groups what one camera sees into events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventRules {
    /// How long after the camera last saw something a new sighting still joins its event, in
    /// milliseconds; never less than the quiet.
    pub window_ms: u64,
    /// How long the camera sees nothing before a sighting ends, in milliseconds; never less than
    /// [`QUIET_FOR_MS`]. An event video's post-roll, where events record videos.
    pub quiet_ms: u64,
    /// Whether plain motion keeps a sighting going between the detections of a camera whose AI
    /// tells things apart, for up to [`MOTION_HOLDS_MS`] after the last. It never starts one.
    pub motion_continues: bool,
}

impl EventRules {
    /// Events open for `window_ms` after the camera last saw something, each sighting ending
    /// after [`QUIET_FOR_MS`] of nothing, and only what the camera tells apart counting.
    pub fn grouping(window_ms: u64) -> Self {
        Self {
            window_ms,
            quiet_ms: QUIET_FOR_MS,
            motion_continues: false,
        }
    }
}

/// `tags` as one file's tags: each once, in the order the app lists what cameras see
/// ([`Detection::ALL`]'s, then [`MOTION_TAG`]), any other last as it came.
pub fn merged_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let rank = |tag: &str| {
        Detection::ALL
            .iter()
            .position(|detection| detection.key() == tag)
            .unwrap_or(if tag == MOTION_TAG {
                Detection::ALL.len()
            } else {
                Detection::ALL.len() + 1
            })
    };
    let mut merged: Vec<String> = Vec::new();
    for tag in tags {
        if !tag.is_empty() && !merged.iter().any(|held| held == tag) {
            merged.push(tag.to_string());
        }
    }
    merged.sort_by_key(|tag| rank(tag));
    merged
}

/// The tag of a file saved for an event of a camera that knows only motion.
pub const MOTION_TAG: &str = "motion";

/// Something a camera saw, from when it came into view until the window after it last saw it
/// passed, and what the app saved of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraEvent {
    /// Its id, `<started>-<camera id>`: no other event of the camera's starts in the same
    /// millisecond.
    pub id: String,
    /// The camera that saw it.
    pub camera: CameraId,
    /// When it started, in milliseconds since 1970.
    pub started: u64,
    /// When the camera last saw it, in milliseconds since 1970; `None` while it sees it now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<u64>,
    /// Whether the camera saw motion during it.
    #[serde(default)]
    pub motion: bool,
    /// Everything the camera's AI told apart during it, in [`Detection::ALL`]'s order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected: Vec<Detection>,
    /// How many times the camera saw something during it, each after a quiet of at least the
    /// tracker's ([`EventRules::quiet_ms`]): 1 for a car that passes once.
    pub sightings: u32,
    /// The snapshot saved as it started, by its name in the snapshots folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// The recordings made during it, by their names in the videos folder, oldest first: one a
    /// sighting, or more where the stream reconnected during one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub videos: Vec<String>,
}

impl CameraEvent {
    /// Whether the camera sees it now.
    pub fn ongoing(&self) -> bool {
        self.ended.is_none()
    }

    /// Every file it saved, its snapshot first, by name.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.snapshot.iter().chain(&self.videos).map(String::as_str)
    }
}

/// What one answer did to an event, for the program that acts on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStep {
    /// The camera saw something with no event open: the event's picture, and its first sighting.
    Started,
    /// It saw something again within the window: another sighting of the same event.
    SeenAgain,
    /// It saw something new during a sighting: nothing new to save.
    Added,
    /// It has seen nothing for the quiet ([`EventRules::quiet_ms`]): the sighting is over, and
    /// the event stays open for the rest of the window.
    Quiet,
    /// It has seen nothing for the whole window, or its watch stopped: the event is over, and
    /// what the camera sees next starts another.
    Closed,
}

/// Turns what one camera sees, answer by answer, into events.
#[derive(Debug)]
pub struct EventTracker {
    camera: CameraId,
    /// Whether the camera's AI tells things apart, so plain motion doesn't count.
    tells_apart: bool,
    /// How long after the camera last saw something a new sighting still joins its event, in
    /// milliseconds; never less than `quiet_ms`.
    window_ms: u64,
    /// How long the camera sees nothing before a sighting ends, in milliseconds; never less than
    /// [`QUIET_FOR_MS`].
    quiet_ms: u64,
    /// Whether plain motion keeps a sighting going between the AI's detections.
    motion_continues: bool,
    /// The event open, and when the camera last saw it.
    current: Option<(CameraEvent, u64)>,
    /// When the camera last saw something its events count, which plain motion holds a sighting
    /// open after for up to [`MOTION_HOLDS_MS`].
    last_counted: u64,
}

impl EventTracker {
    /// A tracker for `camera`, whose AI tells things apart when `tells_apart`, that groups what
    /// it sees by `rules`. A quiet shorter than [`QUIET_FOR_MS`] is that long, and a window
    /// shorter than the quiet is as long as it: an event then ends with its first sighting.
    pub fn new(camera: CameraId, tells_apart: bool, rules: EventRules) -> Self {
        let quiet_ms = rules.quiet_ms.max(QUIET_FOR_MS);
        Self {
            camera,
            tells_apart,
            window_ms: rules.window_ms.max(quiet_ms),
            quiet_ms,
            motion_continues: rules.motion_continues,
            current: None,
            last_counted: 0,
        }
    }

    /// Whether `activity` is something this camera's events count.
    fn sees(&self, activity: &CameraActivity) -> bool {
        if self.tells_apart {
            !activity.detected.is_empty()
        } else {
            activity.motion
        }
    }

    /// What `activity` shows as the tags of a file saved now: the keys of what the camera's AI
    /// tells apart in it, in [`Detection::ALL`]'s order, or [`MOTION_TAG`] for a camera that
    /// knows only motion; none when it shows nothing this camera's events count.
    pub fn tags(&self, activity: &CameraActivity) -> Vec<&'static str> {
        if !self.sees(activity) {
            return Vec::new();
        }
        if !self.tells_apart {
            return vec![MOTION_TAG];
        }
        Detection::ALL
            .into_iter()
            .filter(|detection| activity.detected.contains(detection))
            .map(Detection::key)
            .collect()
    }

    /// Takes what the camera sees at `now`, in milliseconds since 1970. Returns what this answer
    /// did and the event as it is now, when it changed anything; `None` when nothing changed.
    pub fn see(&mut self, activity: &CameraActivity, now: u64) -> Option<(EventStep, CameraEvent)> {
        let counted = self.sees(activity);
        if counted {
            self.last_counted = now;
        }
        // Plain motion holds a sighting the AI started, for a while after it last saw something.
        let held = self.motion_continues
            && self.tells_apart
            && activity.motion
            && now.saturating_sub(self.last_counted) < MOTION_HOLDS_MS;
        let Some((event, last_seen)) = &mut self.current else {
            let sees = counted;
            if !sees {
                return None;
            }
            let mut detected = activity.detected.clone();
            detected.sort();
            detected.dedup();
            let event = CameraEvent {
                id: format!("{now}-{}", self.camera),
                camera: self.camera.clone(),
                started: now,
                ended: None,
                motion: activity.motion,
                detected,
                sightings: 1,
                snapshot: None,
                videos: Vec::new(),
            };
            self.current = Some((event.clone(), now));
            return Some((EventStep::Started, event));
        };

        // Motion goes on holding a sighting while it lasts, never one that is over.
        let sees = counted || (held && event.ended.is_none());
        if !sees {
            let quiet = now.saturating_sub(*last_seen);
            if quiet >= self.window_ms {
                return self.end().map(|event| (EventStep::Closed, event));
            }
            if quiet >= self.quiet_ms && event.ended.is_none() {
                event.ended = Some(*last_seen);
                return Some((EventStep::Quiet, event.clone()));
            }
            return None;
        }

        *last_seen = now;
        let again = event.ended.take().is_some();
        if again {
            event.sightings += 1;
        }
        let before = (event.motion, event.detected.len());
        event.motion |= activity.motion;
        for detection in &activity.detected {
            if !event.detected.contains(detection) {
                event.detected.push(*detection);
            }
        }
        // `Detection`'s order is `Detection::ALL`'s.
        event.detected.sort();
        let step = if again {
            EventStep::SeenAgain
        } else if before != (event.motion, event.detected.len()) {
            EventStep::Added
        } else {
            return None;
        };
        Some((step, event.clone()))
    }

    /// Ends the event open, as the camera last saw it: for when the window passes with nothing
    /// seen, or the watch stops asking.
    pub fn end(&mut self) -> Option<CameraEvent> {
        let (mut event, last_seen) = self.current.take()?;
        event.ended.get_or_insert(last_seen);
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;

    fn sees(motion: bool, detected: &[Detection]) -> CameraActivity {
        CameraActivity {
            motion,
            detected: detected.to_vec(),
            spotlight: None,
        }
    }

    #[test]
    fn a_sighting_lasts_until_the_camera_has_seen_nothing_for_a_while() {
        // The shortest window groups nothing: the event ends with its sighting.
        let mut tracker = EventTracker::new(CameraId::from("cam"), true, EventRules::grouping(0));
        assert_eq!(tracker.see(&sees(false, &[]), 1_000), None);

        let (step, started) = tracker
            .see(&sees(true, &[Detection::Person]), 2_000)
            .expect("a person starts an event");
        assert_eq!(step, EventStep::Started);
        assert_eq!(started.id, "2000-cam");
        assert!(started.ongoing());
        assert_eq!(started.detected, [Detection::Person]);
        assert_eq!(started.sightings, 1);

        // Seeing the same again changes nothing; a gap shorter than the quiet keeps it going.
        assert_eq!(tracker.see(&sees(true, &[Detection::Person]), 3_000), None);
        assert_eq!(tracker.see(&sees(false, &[]), 4_000), None);
        let (step, added) = tracker
            .see(&sees(true, &[Detection::Vehicle]), 5_000)
            .expect("a vehicle adds to it");
        assert_eq!(step, EventStep::Added);
        assert_eq!(added.id, "2000-cam");
        assert_eq!(added.detected, [Detection::Person, Detection::Vehicle]);

        assert_eq!(
            tracker.see(&sees(false, &[]), 5_000 + QUIET_FOR_MS - 1),
            None
        );
        let (step, ended) = tracker
            .see(&sees(false, &[]), 5_000 + QUIET_FOR_MS)
            .expect("the quiet ends it");
        assert_eq!(step, EventStep::Closed);
        assert_eq!(ended.ended, Some(5_000), "as the camera last saw it");

        let (step, next) = tracker
            .see(&sees(false, &[Detection::Animal]), 20_000)
            .expect("another starts");
        assert_eq!(step, EventStep::Started);
        assert_eq!(next.id, "20000-cam");
    }

    #[test]
    fn what_the_camera_sees_again_within_the_window_joins_its_event() {
        let mut tracker = EventTracker::new(
            CameraId::from("cam"),
            true,
            EventRules::grouping(5 * MINUTE),
        );
        let (step, first) = tracker.see(&sees(true, &[Detection::Vehicle]), 0).unwrap();
        assert_eq!(step, EventStep::Started);

        // Ten seconds of nothing ends the sighting, not the event.
        let (step, quiet) = tracker.see(&sees(false, &[]), QUIET_FOR_MS).unwrap();
        assert_eq!((step, quiet.ended), (EventStep::Quiet, Some(0)));
        assert_eq!(tracker.see(&sees(false, &[]), MINUTE), None, "said once");

        // Two minutes on, the car is back with its driver: the same event, seen twice.
        let (step, again) = tracker
            .see(
                &sees(true, &[Detection::Person, Detection::Vehicle]),
                2 * MINUTE,
            )
            .unwrap();
        assert_eq!(step, EventStep::SeenAgain);
        assert_eq!(again.id, first.id);
        assert_eq!(again.sightings, 2);
        assert!(again.ongoing(), "the camera sees it again");
        assert_eq!(again.detected, [Detection::Person, Detection::Vehicle]);

        // The window runs from the last sighting: five minutes of nothing after it closes it.
        let (step, _) = tracker
            .see(&sees(false, &[]), 2 * MINUTE + QUIET_FOR_MS)
            .unwrap();
        assert_eq!(step, EventStep::Quiet);
        assert_eq!(tracker.see(&sees(false, &[]), 7 * MINUTE - 1), None);
        let (step, closed) = tracker.see(&sees(false, &[]), 7 * MINUTE).unwrap();
        assert_eq!(step, EventStep::Closed);
        assert_eq!(closed.ended, Some(2 * MINUTE));

        // The next car is an event of its own.
        let (step, next) = tracker
            .see(&sees(true, &[Detection::Vehicle]), 8 * MINUTE)
            .unwrap();
        assert_eq!(step, EventStep::Started);
        assert_ne!(next.id, first.id);
        assert_eq!(next.sightings, 1);
    }

    #[test]
    fn plain_motion_counts_only_for_a_camera_that_tells_nothing_apart() {
        let mut smart = EventTracker::new(CameraId::from("ai"), true, EventRules::grouping(MINUTE));
        assert_eq!(smart.see(&sees(true, &[]), 1_000), None);
        assert!(smart.tags(&sees(true, &[])).is_empty());

        let mut plain =
            EventTracker::new(CameraId::from("plain"), false, EventRules::grouping(MINUTE));
        let (_, started) = plain
            .see(&sees(true, &[]), 1_000)
            .expect("motion starts one");
        assert!(started.motion);
        assert!(started.detected.is_empty());
        assert_eq!(plain.tags(&sees(true, &[])), [MOTION_TAG]);
    }

    #[test]
    fn a_files_tags_are_what_the_camera_tells_apart_now() {
        let tracker = EventTracker::new(CameraId::from("cam"), true, EventRules::grouping(MINUTE));
        assert_eq!(
            tracker.tags(&sees(true, &[Detection::Animal, Detection::Person])),
            ["person", "animal"],
            "in the order the app lists them, without plain motion"
        );
        assert!(tracker.tags(&sees(false, &[])).is_empty());
    }

    #[test]
    fn ending_a_watch_ends_its_event_as_last_seen() {
        let mut tracker =
            EventTracker::new(CameraId::from("cam"), false, EventRules::grouping(MINUTE));
        assert_eq!(tracker.end(), None);
        tracker.see(&sees(true, &[]), 1_000);
        tracker.see(&sees(true, &[]), 2_000);
        let ended = tracker.end().expect("it was going on");
        assert_eq!((ended.started, ended.ended), (1_000, Some(2_000)));
        assert_eq!(tracker.end(), None);

        // One already quiet keeps the end it had.
        tracker.see(&sees(true, &[]), 100_000);
        tracker.see(&sees(false, &[]), 100_000 + QUIET_FOR_MS);
        assert_eq!(tracker.end().unwrap().ended, Some(100_000));
    }

    #[test]
    fn an_event_is_saved_without_what_it_lacks() {
        let event = CameraEvent {
            id: "1-cam".into(),
            camera: CameraId::from("cam"),
            started: 1,
            ended: None,
            motion: true,
            detected: Vec::new(),
            sightings: 1,
            snapshot: None,
            videos: Vec::new(),
        };
        let text = serde_json::to_string(&event).unwrap();
        assert_eq!(
            text,
            r#"{"id":"1-cam","camera":"cam","started":1,"motion":true,"sightings":1}"#
        );
        assert_eq!(serde_json::from_str::<CameraEvent>(&text).unwrap(), event);
    }

    #[test]
    fn an_events_files_are_its_snapshot_then_its_videos() {
        let event = CameraEvent {
            id: "1-cam".into(),
            camera: CameraId::from("cam"),
            started: 1,
            ended: Some(2),
            motion: false,
            detected: vec![Detection::Vehicle],
            sightings: 2,
            snapshot: Some("snapshot-1-cam.vehicle.png".into()),
            videos: vec![
                "video-1-cam.vehicle.mp4".into(),
                "video-9-cam.vehicle.mp4".into(),
            ],
        };
        assert_eq!(
            event.files().collect::<Vec<_>>(),
            [
                "snapshot-1-cam.vehicle.png",
                "video-1-cam.vehicle.mp4",
                "video-9-cam.vehicle.mp4"
            ]
        );
    }

    /// Rules as an event video's: a 30 s post-roll, five-minute events, and motion as asked.
    fn video_rules(motion_continues: bool) -> EventRules {
        EventRules {
            window_ms: 5 * MINUTE,
            quiet_ms: 30_000,
            motion_continues,
        }
    }

    #[test]
    fn a_sighting_lasts_the_post_roll_so_a_grazing_animal_is_one_sighting() {
        let mut tracker = EventTracker::new(CameraId::from("cam"), true, video_rules(false));
        let animal = sees(false, &[Detection::Animal]);
        let (step, _) = tracker.see(&animal, 0).unwrap();
        assert_eq!(step, EventStep::Started);

        // The AI loses the deer for twenty seconds at a time: still one sighting.
        assert_eq!(tracker.see(&sees(false, &[]), 20_000), None);
        assert_eq!(tracker.see(&animal, 20_000), None);
        assert_eq!(tracker.see(&sees(false, &[]), 49_999), None);
        assert_eq!(tracker.see(&animal, 49_999), None);

        // Thirty seconds of nothing ends it; what comes after is a second sighting.
        let (step, quiet) = tracker.see(&sees(false, &[]), 79_999).unwrap();
        assert_eq!((step, quiet.ended), (EventStep::Quiet, Some(49_999)));
        let (step, again) = tracker.see(&animal, 2 * MINUTE).unwrap();
        assert_eq!((step, again.sightings), (EventStep::SeenAgain, 2));
    }

    #[test]
    fn motion_keeps_a_sighting_going_for_a_while_but_never_starts_one() {
        let mut tracker = EventTracker::new(CameraId::from("cam"), true, video_rules(true));
        let moving = sees(true, &[]);
        assert_eq!(tracker.see(&moving, 0), None, "motion alone starts nothing");

        tracker
            .see(&sees(true, &[Detection::Animal]), 1_000)
            .unwrap();
        // Motion with nothing the AI tells apart holds the sighting past the post-roll...
        assert_eq!(tracker.see(&moving, 40_000), None);
        assert_eq!(tracker.see(&moving, 80_000), None);
        assert_eq!(tracker.see(&moving, 1_000 + MOTION_HOLDS_MS - 1), None);
        // ...until the AI hasn't seen anything for two minutes: then motion no longer counts.
        assert_eq!(tracker.see(&moving, 1_000 + MOTION_HOLDS_MS), None);
        let (step, quiet) = tracker
            .see(&moving, 1_000 + MOTION_HOLDS_MS - 1 + 30_000)
            .unwrap();
        assert_eq!(step, EventStep::Quiet);
        assert_eq!(quiet.ended, Some(1_000 + MOTION_HOLDS_MS - 1));

        // Once the sighting is over, motion doesn't bring it back.
        assert_eq!(tracker.see(&moving, 200_000), None);
    }

    #[test]
    fn without_motion_continuing_motion_holds_nothing() {
        let mut tracker = EventTracker::new(CameraId::from("cam"), true, video_rules(false));
        tracker.see(&sees(false, &[Detection::Person]), 0).unwrap();
        assert_eq!(tracker.see(&sees(true, &[]), 10_000), None);
        let (step, quiet) = tracker.see(&sees(true, &[]), 30_000).unwrap();
        assert_eq!((step, quiet.ended), (EventStep::Quiet, Some(0)));
    }

    #[test]
    fn a_quiet_shorter_than_the_least_is_the_least() {
        let rules = EventRules {
            window_ms: 0,
            quiet_ms: 1,
            motion_continues: false,
        };
        let mut tracker = EventTracker::new(CameraId::from("cam"), false, rules);
        tracker.see(&sees(true, &[]), 0).unwrap();
        assert_eq!(tracker.see(&sees(false, &[]), QUIET_FOR_MS - 1), None);
        let (step, _) = tracker.see(&sees(false, &[]), QUIET_FOR_MS).unwrap();
        assert_eq!(
            step,
            EventStep::Closed,
            "the window is as long as the quiet"
        );
    }

    #[test]
    fn merged_tags_keep_each_once_in_the_apps_order() {
        assert_eq!(
            merged_tags(["animal", "person", "animal", "vehicle"]),
            ["person", "vehicle", "animal"]
        );
        assert_eq!(merged_tags(["motion", "", "person"]), ["person", "motion"]);
        assert!(merged_tags([]).is_empty());
    }
}
