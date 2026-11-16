//! PTZ through ONVIF's PTZ service: `ContinuousMove`, which lasts until a `Stop`, for a camera
//! that is neither Dahua's (DVRIP) nor Reolink's (its own API). A light bulb camera on a pan and
//! tilt base is one.
//!
//! The probe logs in once, finds the PTZ and media services, the media profile a move names, and
//! whether the camera pans, tilts or zooms. A refused login is final: the client remembers it and
//! sends nothing after it.

use std::{thread, time::Duration};

use super::{Driver, PtzVelocity};
use crate::{
    camera_config::CameraConfig,
    onvif::{OnvifClient, OnvifError},
};

/// What a move needs: the logged-in client, the PTZ service's path and the profile a move names.
struct Ready {
    client: OnvifClient,
    ptz: String,
    profile: String,
}

/// PTZ on a camera over ONVIF.
pub struct OnvifPtz {
    camera: CameraConfig,
    ready: Option<Ready>,
    /// Why the camera refused the login, which is never tried again.
    refused: bool,
    status: String,
}

impl OnvifPtz {
    /// A client for `camera`'s ONVIF port. It logs in on the first [`Driver::probe`].
    pub fn new(camera: &CameraConfig) -> Self {
        Self {
            camera: camera.clone(),
            ready: None,
            refused: false,
            status: "ONVIF PTZ: not probed.".to_string(),
        }
    }

    /// Logs in and finds what a move needs; `Ok(false)` for a camera with nothing to move.
    fn open(&mut self) -> Result<bool, OnvifError> {
        let camera = &self.camera;
        let (mut client, _) = OnvifClient::connect(
            &camera.host,
            camera.onvif_port,
            &camera.onvif_device_path,
            &camera.user,
            &camera.password,
        )?;
        let services = client.services()?;
        let (Some(ptz), Some(media)) = (services.ptz, services.media) else {
            return Ok(false);
        };
        let node = client.ptz_node(&ptz)?;
        if !node.pan_tilt && !node.zoom {
            return Ok(false);
        }
        let profile = client.profile_token(&media)?;
        self.ready = Some(Ready {
            client,
            ptz,
            profile,
        });
        Ok(true)
    }

    /// Says why a call failed, and forgets a connection that broke or a login that was refused.
    fn failed(&mut self, error: OnvifError) {
        self.status = format!("ONVIF PTZ: {error}");
        match error {
            OnvifError::LoginRejected(_) => {
                self.refused = true;
                self.ready = None;
            }
            OnvifError::Unreachable(_) => self.ready = None,
            _ => {}
        }
    }

    /// Sends a move, or a stop for a still velocity, logging in again first when the connection
    /// broke; whether the camera took it.
    fn send(&mut self, velocity: PtzVelocity) -> bool {
        if self.refused {
            return false;
        }
        if self.ready.is_none() {
            match self.open() {
                Ok(true) => {}
                Ok(false) => return false,
                Err(error) => {
                    self.failed(error);
                    return false;
                }
            }
        }
        let Some(ready) = self.ready.as_mut() else {
            return false;
        };
        let sent = if velocity.is_still() {
            ready.client.stop(&ready.ptz, &ready.profile)
        } else {
            ready
                .client
                .continuous_move(&ready.ptz, &ready.profile, velocity)
        };
        match sent {
            Ok(()) => true,
            Err(error) => {
                self.failed(error);
                false
            }
        }
    }
}

impl Driver for OnvifPtz {
    fn probe(&mut self) -> bool {
        // A login with no password can only fail, and failures count towards locking the account.
        if self.camera.password.is_empty() {
            self.status = "ONVIF PTZ: skipped because no password is set".to_string();
            return false;
        }
        let port = self.camera.onvif_port;
        match self.open() {
            Ok(true) => {
                self.status = format!("ONVIF PTZ ready on port {port}");
                true
            }
            Ok(false) => {
                self.status = format!("This camera has no PTZ over ONVIF (port {port}).");
                false
            }
            Err(OnvifError::Unreachable(_)) => {
                self.status = super::unreachable("ONVIF", port);
                false
            }
            Err(error) => {
                self.failed(error);
                false
            }
        }
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn move_by(&mut self, velocity: PtzVelocity) -> bool {
        // A still velocity is a stop, which `PtzSession` sends as one.
        velocity.is_still() || self.send(velocity)
    }

    fn stop_with_retries(&mut self) -> bool {
        for attempt in 1..=3 {
            if self.send(PtzVelocity::default()) {
                return true;
            }
            if self.refused {
                return false;
            }
            if attempt < 3 {
                thread::sleep(Duration::from_millis(200));
            }
        }
        false
    }
}
