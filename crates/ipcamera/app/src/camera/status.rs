//! What the status line says about a stream, and what its tiles say over its picture when it
//! lost the camera or gave up.

use ipcamera_core::CameraConfig;
use ipcamera_media::MediaStatus;

use crate::bridge::{StatusTone, StreamTrouble};

/// What a stream's tiles say over its picture: whether it is in trouble, and what happened as a
/// sentence (`""` while it isn't). Connecting for the first time isn't trouble: the tile's
/// numbers already say it waits for a picture.
pub fn trouble(status: &MediaStatus) -> (StreamTrouble, String) {
    match status {
        MediaStatus::Reconnecting { attempt } => (
            StreamTrouble::Reconnecting,
            format!("Reconnecting (attempt {attempt})\u{2026}"),
        ),
        MediaStatus::Failed(why) if status.login_rejected() => {
            (StreamTrouble::Refused, sentence(why))
        }
        MediaStatus::Failed(why) => (StreamTrouble::Stopped, sentence(why)),
        MediaStatus::Connecting | MediaStatus::Live | MediaStatus::Stopped => {
            (StreamTrouble::Fine, String::new())
        }
    }
}

/// A reason the media stack gives (`the camera rejected the username or password`) as a sentence
/// of its own: capitalized, with a full stop.
fn sentence(why: &str) -> String {
    let why = why.trim();
    let mut letters = why.chars();
    let Some(first) = letters.next() else {
        return "The stream stopped.".to_string();
    };
    let ending = if why.ends_with(['.', '!', '?']) {
        ""
    } else {
        "."
    };
    format!("{}{}{ending}", first.to_uppercase(), letters.as_str())
}

/// The pipeline's status as words and a tone.
pub fn describe(status: &MediaStatus, camera: &CameraConfig) -> (String, StatusTone) {
    match status {
        MediaStatus::Connecting => (connecting(camera), StatusTone::Normal),
        MediaStatus::Live => ("Live.".to_string(), StatusTone::Ok),
        MediaStatus::Reconnecting { attempt } => (
            format!("Reconnecting to the camera (attempt {attempt})\u{2026}"),
            StatusTone::Warn,
        ),
        MediaStatus::Stopped => ("Idle.".to_string(), StatusTone::Normal),
        MediaStatus::Failed(message) => (format!("Stream failed: {message}"), StatusTone::Warn),
    }
}

fn connecting(camera: &CameraConfig) -> String {
    format!(
        "Connecting to {}:{} as {}\u{2026}",
        camera.host, camera.rtsp_port, camera.user
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connecting_names_where_it_is_going() {
        let camera = CameraConfig {
            host: "10.0.0.5".into(),
            rtsp_port: 8554,
            user: "viewer".into(),
            ..CameraConfig::default()
        };

        let (text, tone) = describe(&MediaStatus::Connecting, &camera);

        assert_eq!(text, "Connecting to 10.0.0.5:8554 as viewer\u{2026}");
        assert_eq!(tone, StatusTone::Normal);
    }

    #[test]
    fn trouble_reads_as_a_warning_and_live_as_good() {
        let camera = CameraConfig::default();

        assert_eq!(
            describe(&MediaStatus::Live, &camera),
            ("Live.".to_string(), StatusTone::Ok)
        );
        assert_eq!(
            describe(&MediaStatus::Reconnecting { attempt: 3 }, &camera),
            (
                "Reconnecting to the camera (attempt 3)\u{2026}".to_string(),
                StatusTone::Warn
            )
        );
        assert_eq!(
            describe(&MediaStatus::Failed("no route".into()), &camera),
            ("Stream failed: no route".to_string(), StatusTone::Warn)
        );
    }

    #[test]
    fn a_tile_says_nothing_while_the_stream_connects_or_plays() {
        for status in [
            MediaStatus::Connecting,
            MediaStatus::Live,
            MediaStatus::Stopped,
        ] {
            assert_eq!(trouble(&status), (StreamTrouble::Fine, String::new()));
        }
    }

    #[test]
    fn a_tile_says_the_stream_reconnects_and_on_which_try() {
        assert_eq!(
            trouble(&MediaStatus::Reconnecting { attempt: 3 }),
            (
                StreamTrouble::Reconnecting,
                "Reconnecting (attempt 3)\u{2026}".to_string()
            )
        );
    }

    #[test]
    fn a_tile_says_why_the_stream_stopped_as_a_sentence() {
        assert_eq!(
            trouble(&MediaStatus::Failed(
                "the camera sends no H.264 or H.265 video".into()
            )),
            (
                StreamTrouble::Stopped,
                "The camera sends no H.264 or H.265 video.".to_string()
            )
        );
        assert_eq!(
            trouble(&MediaStatus::Failed("it ended.".into())).1,
            "It ended.",
            "a reason that ends its sentence keeps its own stop"
        );
        assert_eq!(
            trouble(&MediaStatus::Failed(String::new())).1,
            "The stream stopped."
        );
    }

    #[test]
    fn a_refused_login_is_told_apart_for_the_settings() {
        let refused = MediaStatus::Failed("the camera rejected the username or password".into());
        assert!(refused.login_rejected());
        assert_eq!(
            trouble(&refused),
            (
                StreamTrouble::Refused,
                "The camera rejected the username or password.".to_string()
            )
        );
    }
}
