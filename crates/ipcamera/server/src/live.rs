//! One viewer's WebSocket: JSON `config` and `status` messages, and each picture as binary.
//!
//! A picture is a flags byte (bit 0: keyframe), its timestamp in microseconds as 8 big-endian
//! bytes, then the access unit in Annex-B form.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ipcamera_core::{CameraConfig, CameraId, TileStream};
use ipcamera_media::{MediaStatus, Picture, Relayed, VideoConfig};
use serde::{Deserialize, Serialize};

use crate::{Server, streams::Streams};

const STATUS_EVERY: Duration = Duration::from_millis(250);

const KEYFRAME: u8 = 1;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Text<'a> {
    Config {
        codec: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    },
    Status {
        state: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'a str>,
    },
}

impl Text<'_> {
    fn message(&self) -> Message {
        let json = serde_json::to_string(self).expect("these fields always serialize");
        Message::Text(json.into())
    }
}

#[derive(Deserialize)]
pub(crate) struct LiveQuery {
    #[serde(default)]
    stream: String,
}

pub(crate) async fn live(
    State(server): State<Server>,
    Path(id): Path<String>,
    Query(query): Query<LiveQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(camera) = server.library.camera(&CameraId::from(id)) else {
        return (StatusCode::NOT_FOUND, "There is no camera with that id.").into_response();
    };
    let picked = TileStream::played(&query.stream);
    let streams = server.streams.clone();
    upgrade.on_upgrade(move |socket| watch(socket, streams, camera, picked))
}

async fn watch(
    mut socket: WebSocket,
    streams: Arc<Streams>,
    camera: CameraConfig,
    picked: Option<TileStream>,
) {
    let key = (camera.id.clone(), picked);
    let stream = match streams.open(&camera, key.1.as_ref()) {
        Ok(stream) => stream,
        Err(reason) => {
            let _ = socket
                .send(status(&MediaStatus::Failed(reason)).message())
                .await;
            return;
        }
    };
    let mut viewer = stream.feed().watch();
    let mut told: Option<MediaStatus> = None;
    let mut tick = tokio::time::interval(STATUS_EVERY);

    loop {
        let message = tokio::select! {
            next = viewer.next() => match next {
                Some(Relayed::Config(config)) => config_message(&config),
                Some(Relayed::Picture(picture)) => picture_message(&picture),
                None => break,
            },
            _ = tick.tick() => {
                // The camera changed or went: the player reconnects to it as it is now.
                if !streams.current(&key, &stream) {
                    break;
                }
                let now = stream.status();
                if told.as_ref() == Some(&now) {
                    continue;
                }
                let message = status(&now).message();
                told = Some(now);
                message
            },
            incoming = socket.recv() => match incoming {
                // The player sends nothing but the close; axum answers pings itself.
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => continue,
            },
        };
        if socket.send(message).await.is_err() {
            break;
        }
    }
}

fn config_message(config: &VideoConfig) -> Message {
    Text::Config {
        codec: &config.codec,
        width: config.size.map(|(width, _)| width),
        height: config.size.map(|(_, height)| height),
    }
    .message()
}

fn status(status: &MediaStatus) -> Text<'_> {
    let (state, attempt, reason) = match status {
        MediaStatus::Connecting => ("connecting", None, None),
        MediaStatus::Live => ("live", None, None),
        MediaStatus::Reconnecting { attempt } => ("reconnecting", Some(*attempt), None),
        MediaStatus::Stopped => ("stopped", None, None),
        MediaStatus::Failed(reason) => ("failed", None, Some(reason.as_str())),
    };
    Text::Status {
        state,
        attempt,
        reason,
    }
}

fn picture_message(picture: &Picture) -> Message {
    let mut bytes = Vec::with_capacity(9 + picture.bytes.len());
    bytes.push(if picture.keyframe { KEYFRAME } else { 0 });
    bytes.extend_from_slice(&picture.timestamp_us.to_be_bytes());
    bytes.extend_from_slice(&picture.bytes);
    Message::Binary(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(message: Message) -> String {
        match message {
            Message::Text(text) => text.to_string(),
            other => panic!("text, not {other:?}"),
        }
    }

    #[test]
    fn the_json_says_what_the_player_reads() {
        assert_eq!(
            text(config_message(&VideoConfig {
                codec: "avc1.42C00A".into(),
                size: Some((64, 48)),
            })),
            r#"{"type":"config","codec":"avc1.42C00A","width":64,"height":48}"#
        );
        assert_eq!(
            text(config_message(&VideoConfig {
                codec: "hev1.1.6.L93.B0".into(),
                size: None,
            })),
            r#"{"type":"config","codec":"hev1.1.6.L93.B0"}"#
        );
        assert_eq!(
            text(status(&MediaStatus::Reconnecting { attempt: 2 }).message()),
            r#"{"type":"status","state":"reconnecting","attempt":2}"#
        );
        assert_eq!(
            text(
                status(&MediaStatus::Failed(
                    "the camera rejected the username or password".into()
                ))
                .message()
            ),
            r#"{"type":"status","state":"failed","reason":"the camera rejected the username or password"}"#
        );
    }

    #[test]
    fn a_picture_leads_with_its_flags_and_time() {
        let Message::Binary(bytes) = picture_message(&Picture {
            keyframe: true,
            timestamp_us: 0x0102_0304_0506_0708,
            bytes: vec![0, 0, 0, 1, 0x65],
        }) else {
            panic!("pictures go as binary");
        };
        assert_eq!(
            &bytes[..],
            [1, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 1, 0x65].as_slice()
        );
    }
}
