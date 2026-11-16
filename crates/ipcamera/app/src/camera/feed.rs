//! Where one camera's frames come from while it is connected: [`Feed`], a media session or the
//! test picture.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use ipcamera_core::{CameraConfig, sync::LockExt};
use ipcamera_media::{FrameSlot, MediaSession, MediaStatus, RecordRules, RecordStatus};
use ipcamera_storage::{recording, saved_file};

use super::{
    hub::Source,
    pattern::{self, Pattern},
    status,
};
use crate::bridge::StatusTone;

/// A running source of frames.
pub(super) enum Feed {
    Stream(MediaSession),
    Pattern(Pattern),
}

impl Feed {
    pub(super) fn open(source: Source, setting: &CameraConfig) -> Result<Self, String> {
        match source {
            Source::Stream => MediaSession::start(setting)
                .map(Self::Stream)
                .map_err(|error| format!("{error}")),
            Source::Pattern => Ok(Self::Pattern(Pattern::start(pattern::picture_for(
                &setting.name,
            )))),
            Source::Offline => Err("this run has no camera".to_string()),
        }
    }

    /// Where the connection is: the test picture is always live.
    pub(super) fn media_status(&self) -> MediaStatus {
        match self {
            Self::Stream(media) => media.status(),
            Self::Pattern(_) => MediaStatus::Live,
        }
    }

    pub(super) fn frames(&self) -> FrameSlot {
        match self {
            Self::Stream(media) => media.frames(),
            Self::Pattern(pattern) => pattern.frames(),
        }
    }

    pub(super) fn set_gain(&self, gain: f64) {
        if let Self::Stream(media) = self {
            media.set_volume(gain);
        }
    }

    /// Whether a tile shows the pictures. Hidden, a stream decodes nothing and the test picture
    /// draws nothing, and neither keeps a picture.
    pub(super) fn set_shown(&self, shown: bool) {
        match self {
            Self::Stream(media) => media.set_shown(shown),
            Self::Pattern(pattern) => pattern.set_shown(shown),
        }
    }

    pub(super) fn status(&self, setting: &CameraConfig) -> (String, StatusTone) {
        match self {
            Self::Stream(media) => status::describe(&media.status(), setting),
            // A picture standing in for the camera says what the camera's stream would.
            Self::Pattern(pattern) if pattern.is_still() => ("Live.".to_string(), StatusTone::Ok),
            Self::Pattern(_) => ("Test pattern.".to_string(), StatusTone::Ok),
        }
    }

    /// Asks for the picture of the next keyframe even while no tile shows the stream, for an
    /// event's snapshot; the test picture draws one.
    pub(super) fn want_picture(&self) {
        match self {
            Self::Stream(media) => media.want_picture(),
            Self::Pattern(pattern) => pattern.want_picture(),
        }
    }

    /// Records into `folder` as `rules` say, naming each file for camera `id` and the time of its
    /// first picture, tagged with what `tags` holds as it opens (what the camera saw, for a
    /// recording an event started), and giving it a thumbnail. The test picture is drawn here
    /// rather than sent by a camera, so it has nothing to record.
    pub(super) fn start_recording(
        &self,
        folder: PathBuf,
        id: String,
        tags: Arc<Mutex<Vec<String>>>,
        rules: RecordRules,
    ) -> Result<(), String> {
        match self {
            Self::Stream(media) => {
                let frames = media.frames();
                media.start_recording(rules, move |started| {
                    let tags = tags.locked().clone();
                    let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
                    let stamp = saved_file::unix_ms(Ok(started)) / 1_000;
                    let path = recording::file_for(&folder, stamp, &id, &tags);
                    recording::write_thumbnail_soon(path.clone(), frames.clone());
                    path
                });
                Ok(())
            }
            Self::Pattern(_) => Err("The test picture can't be recorded.".to_string()),
        }
    }

    /// Holds back the last `preroll` of the stream for an event's video to start from; `None`
    /// holds nothing. The test picture has nothing to hold.
    pub(super) fn set_preroll(&self, preroll: Option<Duration>) {
        if let Self::Stream(media) = self {
            media.set_preroll(preroll);
        }
    }

    pub(super) fn stop_recording(&self) {
        if let Self::Stream(media) = self {
            media.stop_recording();
        }
    }

    pub(super) fn record_status(&self) -> RecordStatus {
        match self {
            Self::Stream(media) => media.record_status(),
            Self::Pattern(_) => RecordStatus::Off,
        }
    }
}
