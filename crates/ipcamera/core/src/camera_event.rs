//! What a camera saw, as events. [`EventTracker`] turns a watch's answers into [`CameraEvent`]s:
//! each *sighting* ends after a quiet, and sightings within the window join one event. It has no
//! clock of its own: each answer comes with its time.

use serde::{Deserialize, Serialize};

use crate::{CameraActivity, CameraId, Detection, MOTION_HOLDS_MS};

/// The least a camera sees nothing for before a sighting ends, in milliseconds.
pub const QUIET_FOR_MS: u64 = 10_000;

/// How an [`EventTracker`] groups what one camera sees into events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventRules {
    /// How long after the last sighting a new one joins its event, in ms; at least the quiet.
    pub window_ms: u64,
    /// How long of nothing ends a sighting, in ms; at least [`QUIET_FOR_MS`].
    pub quiet_ms: u64,
    /// Whether plain motion keeps an AI camera's sighting going, never starting one.
    pub motion_continues: bool,
}

impl EventRules {
    /// Events open for `window_ms`, with the least quiet and no motion holding.
    pub fn grouping(window_ms: u64) -> Self {
        Self {
            window_ms,
            quiet_ms: QUIET_FOR_MS,
            motion_continues: false,
        }
    }
}

/// `tags` as one file's tags: each once, in [`Detection::ALL`]'s order, then [`MOTION_TAG`].
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

/// Something a camera saw, and what the app saved of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraEvent {
    /// Its id, `<started>-<camera id>`.
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
    /// How many sightings it had.
    pub sightings: u32,
    /// The snapshot saved as it started, by file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// The recordings made during it, by file name, oldest first.
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

/// What one answer did to an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStep {
    /// The camera saw something with no event open.
    Started,
    /// It saw something again within the window: another sighting.
    SeenAgain,
    /// It saw something new during a sighting.
    Added,
    /// It has seen nothing for the quiet: the sighting is over, the event still open.
    Quiet,
    /// It has seen nothing for the whole window, or its watch stopped: the event is over.
    Closed,
}

/// Turns what one camera sees, answer by answer, into events.
#[derive(Debug)]
pub struct EventTracker {
    camera: CameraId,
    /// Whether the camera's AI tells things apart, so plain motion doesn't count.
    tells_apart: bool,
    window_ms: u64,
    quiet_ms: u64,
    motion_continues: bool,
    /// The event open, and when the camera last saw it.
    current: Option<(CameraEvent, u64)>,
    last_counted: u64,
}

impl EventTracker {
    /// A tracker for `camera`; the quiet is raised to [`QUIET_FOR_MS`], the window to the quiet.
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

    fn sees(&self, activity: &CameraActivity) -> bool {
        if self.tells_apart {
            !activity.detected.is_empty()
        } else {
            activity.motion
        }
    }

    /// The tags of a file saved now: what the AI tells apart, or [`MOTION_TAG`] without an AI.
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

    /// Takes what the camera sees at `now`, in ms since 1970; `None` when nothing changed.
    pub fn see(&mut self, activity: &CameraActivity, now: u64) -> Option<(EventStep, CameraEvent)> {
        let counted = self.sees(activity);
        if counted {
            self.last_counted = now;
        }
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

        // Motion holds a sighting while it lasts, never one that is over.
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

    /// Ends the open event, as the camera last saw it.
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

        let (step, quiet) = tracker.see(&sees(false, &[]), QUIET_FOR_MS).unwrap();
        assert_eq!((step, quiet.ended), (EventStep::Quiet, Some(0)));
        assert_eq!(tracker.see(&sees(false, &[]), MINUTE), None, "said once");

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

        let (step, _) = tracker
            .see(&sees(false, &[]), 2 * MINUTE + QUIET_FOR_MS)
            .unwrap();
        assert_eq!(step, EventStep::Quiet);
        assert_eq!(tracker.see(&sees(false, &[]), 7 * MINUTE - 1), None);
        let (step, closed) = tracker.see(&sees(false, &[]), 7 * MINUTE).unwrap();
        assert_eq!(step, EventStep::Closed);
        assert_eq!(closed.ended, Some(2 * MINUTE));

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

        assert_eq!(tracker.see(&sees(false, &[]), 20_000), None);
        assert_eq!(tracker.see(&animal, 20_000), None);
        assert_eq!(tracker.see(&sees(false, &[]), 49_999), None);
        assert_eq!(tracker.see(&animal, 49_999), None);

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
        assert_eq!(tracker.see(&moving, 40_000), None);
        assert_eq!(tracker.see(&moving, 80_000), None);
        assert_eq!(tracker.see(&moving, 1_000 + MOTION_HOLDS_MS - 1), None);
        assert_eq!(tracker.see(&moving, 1_000 + MOTION_HOLDS_MS), None);
        let (step, quiet) = tracker
            .see(&moving, 1_000 + MOTION_HOLDS_MS - 1 + 30_000)
            .unwrap();
        assert_eq!(step, EventStep::Quiet);
        assert_eq!(quiet.ended, Some(1_000 + MOTION_HOLDS_MS - 1));

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
