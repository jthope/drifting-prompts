//! PTZ through ONVIF's PTZ service, for a camera of another brand: `ContinuousMove`, which lasts
//! until a `Stop`.

use std::{thread, time::Duration};

use super::{Driver, PtzVelocity};
use crate::{
    camera_config::CameraConfig,
    onvif::{OnvifClient, OnvifError},
};

struct Ready {
    client: OnvifClient,
    ptz: String,
    profile: String,
}

/// PTZ on a camera over ONVIF.
pub struct OnvifPtz {
    camera: CameraConfig,
    ready: Option<Ready>,
    /// A refused login is never tried again.
    refused: bool,
    status: String,
}

impl OnvifPtz {
    /// A client for `camera`'s ONVIF port, logging in on the first probe.
    pub fn new(camera: &CameraConfig) -> Self {
        Self {
            camera: camera.clone(),
            ready: None,
            refused: false,
            status: "ONVIF PTZ: not probed.".to_string(),
        }
    }

    /// `Ok(false)` for a camera with nothing to move.
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

    /// Sends a move, or a stop for a still velocity, logging in again after a broken connection.
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
        // A still velocity arrives as a stop of its own.
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
