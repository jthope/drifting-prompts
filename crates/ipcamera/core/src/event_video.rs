//! How the videos an event saves are made: [`EventVideo`], Settings → Automation's for every
//! camera, which a camera's own settings can replace as a whole (`CameraConfig::event_video`).
//!
//! A video keeps recording until the camera has seen nothing for [`after_seconds`]
//! (`EventVideo::after_seconds`), so what the camera sees again sooner continues the same file
//! rather than starting another: a deer that grazes in and out of the camera's detection is one
//! video, not five. A camera whose AI tells things apart can let plain motion keep a video going
//! between its detections, though never start one ([`motion_continues`]). Each video starts with
//! the seconds before the camera saw something ([`before_seconds`]), held in memory from a stream
//! that stays connected for it, so an event the camera reports late still shows what it saw. A
//! video that runs longer than [`longest_minutes`] goes on in a new file, from the same keyframe,
//! with nothing lost between them. Only the kinds the user picks save a video ([`kinds`]); every
//! event is logged whatever it saw.
//!
//! The ranges are the app's, checked in one place ([`EventVideo::within_limits`]), so a value a
//! hand edit or an older form left outside them is brought inside rather than refused.
//!
//! [`after_seconds`]: EventVideo::after_seconds
//! [`motion_continues`]: EventVideo::motion_continues
//! [`before_seconds`]: EventVideo::before_seconds
//! [`longest_minutes`]: EventVideo::longest_minutes
//! [`kinds`]: EventVideo::kinds

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Detection, MOTION_TAG, QUIET_FOR_MS, TileStream};

/// The shortest and longest an event's video keeps recording after the camera last saw
/// something, in seconds.
pub const AFTER_SECONDS: (u32, u32) = (10, 120);

/// The shortest and longest stretch from before the camera saw something that a video starts
/// with, in seconds, when it starts with any.
pub const BEFORE_SECONDS: (u32, u32) = (5, 30);

/// The longest a video may run before it goes on in a new file, in minutes.
pub const LONGEST_MINUTES: u32 = 60;

/// How long plain motion keeps a video going after the camera's AI last saw something, in
/// milliseconds: long enough to bridge an animal that drops out of its detection while it grazes,
/// short enough that a tree in the wind can't keep a camera recording all day.
pub const MOTION_HOLDS_MS: u64 = 120_000;

/// How the videos an event saves are made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventVideo {
    /// How long a video keeps recording after the camera last saw something, in seconds: what it
    /// sees again sooner continues the same video. It is also how long the camera sees nothing
    /// before a sighting ends, so an event's count of sightings matches its videos.
    pub after_seconds: u32,
    /// How many seconds from before the camera saw something each video starts with; 0 for none.
    /// More than 0 keeps the camera's stream connected while events are recorded, to hold them.
    pub before_seconds: u32,
    /// The longest a video runs before it goes on in a new file, in minutes; 0 for no limit.
    pub longest_minutes: u32,
    /// Whether plain motion keeps a video going between the AI's detections, for a camera whose AI
    /// tells things apart. It never starts one.
    pub motion_continues: bool,
    /// Which of the camera's streams the videos record: its own, the one its settings name, until
    /// changed.
    pub stream: TileStream,
    /// What the camera has to see for an event to save a video, in [`Detection::ALL`]'s order: all
    /// of them until changed. A camera that knows only motion saves one for motion whatever this
    /// holds, since it can tell nothing apart.
    pub kinds: Vec<Detection>,
}

impl Default for EventVideo {
    fn default() -> Self {
        Self {
            after_seconds: 30,
            before_seconds: 10,
            longest_minutes: 10,
            motion_continues: true,
            stream: TileStream::Own,
            kinds: Detection::ALL.to_vec(),
        }
    }
}

impl EventVideo {
    /// These settings with each value brought within the app's ranges: the post-roll within
    /// [`AFTER_SECONDS`], the pre-roll 0 or within [`BEFORE_SECONDS`], the longest video at most
    /// [`LONGEST_MINUTES`], and the kinds in their order, each once.
    pub fn within_limits(mut self) -> Self {
        let (least, most) = AFTER_SECONDS;
        self.after_seconds = self.after_seconds.clamp(least, most);
        let (least, most) = BEFORE_SECONDS;
        if self.before_seconds > 0 {
            self.before_seconds = self.before_seconds.clamp(least, most);
        }
        self.longest_minutes = self.longest_minutes.min(LONGEST_MINUTES);
        self.kinds.sort();
        self.kinds.dedup();
        self
    }

    /// How long a sighting lasts after the camera last saw something, in milliseconds: the
    /// post-roll, never shorter than [`QUIET_FOR_MS`].
    pub fn quiet_ms(&self) -> u64 {
        (u64::from(self.after_seconds) * 1_000).max(QUIET_FOR_MS)
    }

    /// The stretch each video starts with from before the camera saw something; `None` for none.
    pub fn before(&self) -> Option<Duration> {
        (self.before_seconds > 0).then(|| Duration::from_secs(u64::from(self.before_seconds)))
    }

    /// The longest a video runs before it goes on in a new file; `None` for no limit.
    pub fn longest(&self) -> Option<Duration> {
        (self.longest_minutes > 0)
            .then(|| Duration::from_secs(u64::from(self.longest_minutes) * 60))
    }

    /// Whether what a camera sees, as the tags of a file saved now (`EventTracker::tags`), is
    /// something these settings save a video of: one of [`kinds`](Self::kinds), or motion from a
    /// camera that knows only motion.
    pub fn saves(&self, tags: &[&str]) -> bool {
        tags.iter().any(|tag| {
            *tag == MOTION_TAG
                || Detection::from_key(tag).is_some_and(|kind| self.kinds.contains(&kind))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stream;

    #[test]
    fn the_defaults_keep_thirty_seconds_after_and_ten_before() {
        let video = EventVideo::default();
        assert_eq!(video.quiet_ms(), 30_000);
        assert_eq!(video.before(), Some(Duration::from_secs(10)));
        assert_eq!(video.longest(), Some(Duration::from_secs(600)));
        assert!(video.motion_continues);
        assert_eq!(video.stream, TileStream::Own);
        assert_eq!(video.kinds, Detection::ALL);
    }

    #[test]
    fn values_outside_the_ranges_are_brought_inside() {
        let video = EventVideo {
            after_seconds: 1,
            before_seconds: 2,
            longest_minutes: 600,
            kinds: vec![Detection::Animal, Detection::Person, Detection::Animal],
            ..EventVideo::default()
        }
        .within_limits();
        assert_eq!(video.after_seconds, 10);
        assert_eq!(video.before_seconds, 5);
        assert_eq!(video.longest_minutes, 60);
        assert_eq!(video.kinds, [Detection::Person, Detection::Animal]);

        let none = EventVideo {
            after_seconds: 999,
            before_seconds: 0,
            longest_minutes: 0,
            ..EventVideo::default()
        }
        .within_limits();
        assert_eq!(none.after_seconds, 120);
        assert_eq!(
            none.before(),
            None,
            "0 is no pre-roll, not the least of one"
        );
        assert_eq!(none.longest(), None);
    }

    #[test]
    fn only_the_kinds_picked_save_a_video_and_motion_alone_always_does() {
        let animals = EventVideo {
            kinds: vec![Detection::Animal],
            ..EventVideo::default()
        };
        assert!(animals.saves(&["animal"]));
        assert!(animals.saves(&["person", "animal"]));
        assert!(!animals.saves(&["vehicle"]));
        assert!(!animals.saves(&[]), "nothing seen saves nothing");
        assert!(
            animals.saves(&[MOTION_TAG]),
            "a camera that knows only motion"
        );
    }

    #[test]
    fn it_is_saved_with_the_streams_and_kinds_by_name() {
        let video = EventVideo {
            stream: TileStream::Brand(Stream::Sub),
            kinds: vec![Detection::Person, Detection::Vehicle],
            ..EventVideo::default()
        };
        let text = serde_json::to_string(&video).unwrap();
        assert!(text.contains(r#""stream":"sub""#), "{text}");
        assert!(text.contains(r#""kinds":["person","vehicle"]"#), "{text}");
        assert_eq!(serde_json::from_str::<EventVideo>(&text).unwrap(), video);
        assert_eq!(
            serde_json::from_str::<EventVideo>("{}").unwrap(),
            EventVideo::default(),
            "every field has its default"
        );
    }
}
