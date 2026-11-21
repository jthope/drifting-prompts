//! PTZ through Dahua's HTTP API, a move lasting until its stop. Firmware counts the channel from 0
//! or from 1: one that won't say which is driven from 0, and its first refused move tries 1.

use std::{thread, time::Duration};

use super::{DahuaCommand, Driver, PtzVelocity};
use crate::{
    camera_config::CameraConfig,
    dahua_http::{DahuaHttp, DahuaHttpError},
};

/// PTZ on a Dahua camera over its HTTP API.
pub struct DahuaHttpPtz {
    camera: CameraConfig,
    client: Option<DahuaHttp>,
    channel: u32,
    channel_known: bool,
    last: Option<DahuaCommand>,
    /// A refused login is never tried again.
    refused: bool,
    status: String,
}

impl DahuaHttpPtz {
    /// A client for `camera`'s HTTP API, logging in on the first probe.
    pub fn new(camera: &CameraConfig) -> Self {
        Self {
            camera: camera.clone(),
            client: None,
            channel: camera.channel.max(1) - 1,
            channel_known: false,
            last: None,
            refused: false,
            status: "Dahua HTTP API PTZ: not probed.".to_string(),
        }
    }

    fn client(&mut self) -> Option<&mut DahuaHttp> {
        if self.refused {
            return None;
        }
        if self.client.is_none() {
            let camera = &self.camera;
            match DahuaHttp::connect(
                &camera.host,
                camera.api_port,
                camera.api_https,
                &camera.user,
                &camera.password,
            ) {
                Ok((client, _)) => self.client = Some(client),
                Err(DahuaHttpError::Unreachable(_)) => {
                    self.status = super::unreachable("HTTP API", camera.api_port);
                    return None;
                }
                Err(error) => {
                    self.failed(error);
                    return None;
                }
            }
        }
        self.client.as_mut()
    }

    fn failed(&mut self, error: DahuaHttpError) {
        self.status = format!("Dahua HTTP API PTZ: {error}");
        match error {
            DahuaHttpError::LoginRejected(_) => {
                self.refused = true;
                self.client = None;
            }
            DahuaHttpError::Unreachable(_) => self.client = None,
            _ => {}
        }
    }

    /// A broken connection gets one more try on a new login, and an unknown channel numbering one
    /// try on the other.
    fn send(&mut self, start: bool, command: DahuaCommand) -> bool {
        for attempt in 1..=2 {
            let channel = self.channel;
            let Some(client) = self.client() else {
                return false;
            };
            let sent = if start {
                client.ptz_start(channel, command)
            } else {
                client.ptz_stop(channel, command)
            };
            match sent {
                Ok(()) => {
                    self.channel_known = true;
                    return true;
                }
                Err(DahuaHttpError::Failed(_)) if attempt == 1 && !self.channel_known => {
                    self.channel = self.camera.channel.max(1);
                    self.channel_known = true;
                }
                Err(error @ DahuaHttpError::Unreachable(_)) if attempt == 1 => self.failed(error),
                Err(error) => {
                    self.failed(error);
                    return false;
                }
            }
        }
        false
    }
}

impl Driver for DahuaHttpPtz {
    fn probe(&mut self) -> bool {
        // A login with no password can only fail, and failures count towards locking the account.
        if self.camera.password.is_empty() {
            self.status = "Dahua HTTP API PTZ: skipped because no password is set".to_string();
            return false;
        }
        let (port, camera_channel) = (self.camera.api_port, self.camera.channel);
        let Some(client) = self.client() else {
            return false;
        };
        match client.ptz_caps(camera_channel) {
            Ok(Some(caps)) if !caps.pan_tilt && !caps.zoom => {
                self.status =
                    format!("This camera reports no PTZ over its HTTP API (port {port}).");
                false
            }
            Ok(Some(caps)) => {
                self.channel = caps.channel;
                self.channel_known = true;
                self.status = format!("Dahua HTTP API PTZ ready on port {port}");
                true
            }
            // Some firmware moves without saying what its PTZ does.
            Ok(None) => {
                self.status = format!(
                    "Dahua HTTP API PTZ on port {port} (the camera didn't say what it can do)"
                );
                true
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
        let Some(command) = DahuaCommand::from_velocity(velocity) else {
            return true;
        };
        self.last = Some(command);
        self.send(true, command)
    }

    fn stop_with_retries(&mut self) -> bool {
        let Some(command) = self.last else {
            return true;
        };
        for attempt in 1..=3 {
            if self.send(false, command) {
                self.last = None;
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
