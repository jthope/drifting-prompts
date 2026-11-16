//! PTZ through Dahua's HTTP API (`ptz.cgi`), for a Dahua camera with `http_api` on: one whose
//! DVRIP port is closed, as Amcrest's light bulb cameras', or that has a light the API switches.
//! A move starts with the codes and speeds DVRIP takes ([`DahuaCommand`]) and lasts until its
//! stop.
//!
//! The probe logs in once and asks what the PTZ does, on the channel counted from 0 and then
//! from 1, since firmware differs; a camera that won't say is driven from 0, and its first refused
//! move tries 1. A refused login is final: the client remembers it and sends nothing after it.

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
    /// The channel as `ptz.cgi` counts it.
    channel: u32,
    /// Whether the camera said which numbering it takes; if not, a refused move tries the other.
    channel_known: bool,
    last: Option<DahuaCommand>,
    /// Whether the camera refused the login, which is never tried again.
    refused: bool,
    status: String,
}

impl DahuaHttpPtz {
    /// A client for `camera`'s HTTP API. It logs in on the first [`Driver::probe`].
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

    /// The logged-in client, logging in when there is none; `None` after a refused login or a
    /// camera that didn't answer, with the status saying which.
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

    /// Says why a call failed, and forgets a connection that broke or a login that was refused.
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

    /// Sends a start of `command`, or its stop; whether the camera took it. A broken connection
    /// gets one more try on a new login, and a camera that never said its channel numbering gets
    /// one try on the other.
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
        // A new start replaces the move before it, as over DVRIP.
        self.last = Some(command);
        self.send(true, command)
    }

    fn stop_with_retries(&mut self) -> bool {
        let Some(command) = self.last else {
            return true;
        };
        for attempt in 1..=3 {
            if self.send(false, command) {
                // Stopped: the stop as the session ends has nothing left to stop.
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
