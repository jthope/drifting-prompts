//! PTZ through Reolink's HTTP API: `PtzCtrl`, which starts a move that lasts until a `Stop`.
//!
//! Zoom is the exception, by what the camera says it can do, not by its model. `PtzCtrl`'s zoom
//! is a small fixed step on cameras whose zoom is a position, however long the button is held,
//! which can be too small to see. So a camera that says where its zoom is and how far it goes
//! (`GetZoomFocus` with its range) is zoomed to a position instead (`StartZoomFocus`): each press
//! goes [`ZOOM_STEP`] of the camera's own range further, more at a higher speed, and a held
//! button steps again every [`ZOOM_EVERY`]. A camera that doesn't say keeps `PtzCtrl`'s zoom.
//!
//! A camera with more than one lens zooms one of them, and on another's stream a zoom changes
//! nothing in the picture. Which streams a camera has is the camera's to say: Detect lists them
//! for the camera form.
//!
//! Unlike DVRIP, nothing here holds a session open: the client keeps a token for an hour and
//! logs in again by itself, so a move is one request. A refused login is the client's to remember,
//! and nothing here tries it again.

use std::{thread, time::Duration};

use super::{Driver, PtzVelocity, moves};
use crate::{
    camera_config::CameraConfig,
    reolink::{ReolinkClient, ReolinkError, ZoomPosition},
};

/// How much of the zoom's range one step covers at full speed: at the panel's default speed,
/// half of it, a twentieth of the range, which shows in the picture.
const ZOOM_STEP: f64 = 0.1;

/// How often a held zoom steps again.
const ZOOM_EVERY: Duration = Duration::from_millis(350);

/// PTZ on a Reolink camera, over its HTTP API.
pub struct ReolinkPtz {
    camera: CameraConfig,
    client: Option<ReolinkClient>,
    /// Why the camera refused the login, which is never tried again.
    refused: Option<ReolinkError>,
    /// Where the zoom is and how far it goes, for a camera that says: it is then zoomed by
    /// position.
    zoom: Option<ZoomPosition>,
    /// Whether the move under way is a zoom by position, which a held button steps again.
    stepping: bool,
    status: String,
}

impl ReolinkPtz {
    /// A client for `camera`'s API port. It logs in on the first [`Driver::probe`].
    pub fn new(camera: &CameraConfig) -> Self {
        Self {
            camera: camera.clone(),
            client: None,
            refused: None,
            zoom: None,
            stepping: false,
            status: "Reolink: not probed.".to_string(),
        }
    }

    /// The logged-in client, logging in when there is none. `None` when the camera refused the
    /// login or couldn't be reached, which the status says.
    fn client(&mut self) -> Option<&mut ReolinkClient> {
        if self.refused.is_some() {
            return None;
        }
        if self.client.is_none() {
            let camera = &self.camera;
            match ReolinkClient::connect(
                &camera.host,
                camera.api_port,
                camera.api_https,
                &camera.user,
                &camera.password,
            ) {
                Ok(client) => self.client = Some(client),
                Err(error) => {
                    self.status = match &error {
                        ReolinkError::Unreachable(_) => super::unreachable("API", camera.api_port),
                        _ => format!("Reolink: {error}"),
                    };
                    if matches!(error, ReolinkError::LoginRejected(_) | ReolinkError::Locked) {
                        self.refused = Some(error);
                    }
                    return None;
                }
            }
        }
        self.client.as_mut()
    }

    /// Sends one move or stop, logging in first when needed; whether the camera took it.
    fn send(&mut self, velocity: PtzVelocity) -> bool {
        let channel = self.camera.channel;
        let Some(client) = self.client() else {
            return false;
        };
        let sent = client.ptz_move(channel, velocity);
        self.took(sent)
    }

    /// Zooms one step further from `zoom`, in or out as `velocity` says and further at a higher
    /// speed; whether the camera took it. At either end there is nothing to send.
    fn step_zoom(&mut self, zoom: ZoomPosition, velocity: f64) -> bool {
        let to = zoom.stepped(velocity, ZOOM_STEP * velocity.abs().min(1.0));
        if to == zoom.at {
            return true;
        }
        let channel = self.camera.channel;
        let Some(client) = self.client() else {
            return false;
        };
        let sent = client.zoom_to(channel, to);
        if sent.is_ok() {
            // The next step goes on from where this one is headed, not from where the lens has
            // got to.
            self.zoom = Some(ZoomPosition { at: to, ..zoom });
        }
        self.took(sent)
    }

    /// Whether the camera took what was `sent`; a failure says why in the status, and forgets a
    /// connection that broke or remembers a login that was refused.
    fn took(&mut self, sent: Result<(), ReolinkError>) -> bool {
        let Err(error) = sent else {
            return true;
        };
        self.status = format!("Reolink: {error}");
        match error {
            ReolinkError::LoginRejected(_) | ReolinkError::Locked => {
                self.refused = Some(error);
            }
            // A connection that broke is made again on the next move.
            ReolinkError::Unreachable(_) => self.client = None,
            _ => {}
        }
        false
    }
}

impl Driver for ReolinkPtz {
    fn probe(&mut self) -> bool {
        // A login with no password can only fail, and failures count towards locking the account.
        if self.camera.password.is_empty() {
            self.status = "Reolink: skipped because no password is set".to_string();
            return false;
        }
        let (port, channel) = (self.camera.api_port, self.camera.channel);
        let Some(client) = self.client() else {
            return false;
        };
        match client.features(channel) {
            Ok(features) if features.pan_tilt || features.zoom => {
                // A camera that won't say where its zoom is keeps `PtzCtrl`'s zoom.
                let zoom = client.zoom_position(channel).ok().flatten();
                self.zoom = zoom;
                self.status = format!("Reolink ready on port {port}");
                true
            }
            Ok(_) => {
                self.status = format!("Reolink: no PTZ reported on port {port}");
                false
            }
            Err(error) => {
                self.status = format!("Reolink: {error}");
                false
            }
        }
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn move_by(&mut self, velocity: PtzVelocity) -> bool {
        // A still velocity is a stop, which `PtzSession` sends as one.
        if velocity.is_still() {
            return true;
        }
        if moves(velocity.zoom)
            && let Some(zoom) = self.zoom
        {
            // A new press starts from where the lens is now: it may have been zoomed elsewhere
            // since. A held button's next step goes on from the last one.
            let zoom = if self.stepping {
                zoom
            } else {
                let channel = self.camera.channel;
                self.client()
                    .and_then(|client| client.zoom_position(channel).ok().flatten())
                    .unwrap_or(zoom)
            };
            self.stepping = true;
            return self.step_zoom(zoom, velocity.zoom);
        }
        self.stepping = false;
        self.send(velocity)
    }

    fn repeat_every(&self) -> Option<Duration> {
        self.stepping.then_some(ZOOM_EVERY)
    }

    fn stop_with_retries(&mut self) -> bool {
        // A zoom by position ends by itself where it was sent: there is nothing to stop.
        if std::mem::take(&mut self.stepping) {
            return true;
        }
        for attempt in 1..=3 {
            if self.send(PtzVelocity::default()) {
                return true;
            }
            if self.refused.is_some() {
                return false;
            }
            if attempt < 3 {
                thread::sleep(Duration::from_millis(200));
            }
        }
        false
    }
}
