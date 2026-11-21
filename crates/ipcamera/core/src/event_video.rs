//! How the videos an event saves are made: [`EventVideo`], the app's or a camera's own. Values out
//! of range are brought inside ([`EventVideo::within_limits`]) rather than refused.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Detection, MOTION_TAG, QUIET_FOR_MS, TileStream};

/// The shortest and longest post-roll, in seconds.
pub const AFTER_SECONDS: (u32, u32) = (10, 120);

/// The shortest and longest pre-roll, in seconds, when there is one.
pub const BEFORE_SECONDS: (u32, u32) = (5, 30);

/// The longest a video may run before it goes on in a new file, in minutes.
pub const LONGEST_MINUTES: u32 = 60;

/// How long plain motion keeps a video going after the AI last saw something, in ms: short enough
/// that a tree in the wind can't keep a camera recording all day.
pub const MOTION_HOLDS_MS: u64 = 120_000;

/// How the videos an event saves are made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventVideo {
    /// The post-roll in seconds, which is also the quiet that ends a sighting.
    pub after_seconds: u32,
    /// The pre-roll in seconds; 0 for none. More keeps the stream connected to hold it.
    pub before_seconds: u32,
    /// The longest a video runs before it goes on in a new file, in minutes; 0 for no limit.
    pub longest_minutes: u32,
    /// Whether plain motion keeps a video going between an AI's detections, never starting one.
    pub motion_continues: bool,
    /// Which of the camera's streams the videos record.
    pub stream: TileStream,
    /// What the camera has to see for an event to save a video; motion-only cameras always save.
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
    /// These settings with each value brought within the app's ranges.
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

    /// The post-roll in ms, never shorter than [`QUIET_FOR_MS`].
    pub fn quiet_ms(&self) -> u64 {
        (u64::from(self.after_seconds) * 1_000).max(QUIET_FOR_MS)
    }

    /// The pre-roll; `None` for none.
    pub fn before(&self) -> Option<Duration> {
        (self.before_seconds > 0).then(|| Duration::from_secs(u64::from(self.before_seconds)))
    }

    /// The longest a video runs before it goes on in a new file; `None` for no limit.
    pub fn longest(&self) -> Option<Duration> {
        (self.longest_minutes > 0)
            .then(|| Duration::from_secs(u64::from(self.longest_minutes) * 60))
    }

    /// Whether these settings save a video of what `tags` say the camera sees.
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
