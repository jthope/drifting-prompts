//! PTZ for the web: one connection per camera, and moves that stop by themselves.
//!
//! A lost stop would leave a camera turning, so a move lasts [`MOVE_LASTS`] unless sent again. A
//! camera with no saved password is never tried, since failed logins can lock its account.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ipcamera_core::{
    CameraConfig, CameraId, Library,
    ptz::{PtzSession, PtzVelocity},
    sync::LockExt,
};

use crate::{
    closing::{Closing, OffThread},
    streams::NO_PASSWORD,
};

/// Rides out a late repeat (a held move comes about every 400 ms); a lost stop costs at most this.
pub(crate) const MOVE_LASTS: Duration = Duration::from_secs(1);

struct Held {
    setting: CameraConfig,
    session: OffThread<PtzSession>,
    moving_until: Option<Instant>,
}

impl Held {
    fn session(&self) -> &PtzSession {
        &self.session
    }
}

#[derive(Default)]
pub(crate) struct Ptz {
    held: Mutex<HashMap<CameraId, Held>>,
    closing: Arc<Closing>,
}

impl Ptz {
    pub(crate) fn move_by(
        &self,
        camera: &CameraConfig,
        velocity: PtzVelocity,
        now: Instant,
    ) -> Result<(), String> {
        if camera.password.is_empty() {
            return Err(NO_PASSWORD.to_string());
        }
        let mut held = self.held.locked();
        if held
            .get(&camera.id)
            .is_none_or(|open| open.setting != *camera)
        {
            held.insert(
                camera.id.clone(),
                Held {
                    setting: camera.clone(),
                    session: self.closing.hold(PtzSession::connect(camera)),
                    moving_until: None,
                },
            );
        }
        let open = held
            .get_mut(&camera.id)
            .expect("just made sure it is there");
        open.session().move_by(velocity);
        open.moving_until = (!velocity.is_still()).then_some(now + MOVE_LASTS);
        Ok(())
    }

    pub(crate) fn stop(&self, id: &CameraId) {
        if let Some(open) = self.held.locked().get_mut(id) {
            open.session().stop();
            open.moving_until = None;
        }
    }

    pub(crate) fn expire(&self, now: Instant) {
        for open in self.held.locked().values_mut() {
            if open.moving_until.is_some_and(|until| until <= now) {
                open.session().stop();
                open.moving_until = None;
            }
        }
    }

    pub(crate) fn statuses(&self) -> HashMap<CameraId, String> {
        self.held
            .locked()
            .iter()
            .map(|(id, open)| (id.clone(), open.session().status()))
            .collect()
    }

    pub(crate) fn forget(&self, id: &CameraId) {
        self.held.locked().remove(id);
    }

    pub(crate) fn follow(&self, library: &Library) {
        self.held
            .locked()
            .retain(|id, open| library.camera(id).as_ref() == Some(&open.setting));
    }

    pub(crate) fn stop_all(&self) {
        self.held.locked().clear();
    }

    /// Blocks, so never on the runtime's threads.
    pub(crate) fn stopped_by(&self, until: Instant) -> bool {
        self.closing.wait(until)
    }

    #[cfg(test)]
    fn moving(&self, id: &CameraId) -> bool {
        self.held
            .locked()
            .get(id)
            .is_some_and(|open| open.moving_until.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PTZ port nothing listens on, so the connection never reaches a real camera.
    fn nowhere() -> CameraConfig {
        CameraConfig {
            id: CameraId::from("porch"),
            name: "Porch".into(),
            host: "127.0.0.1".into(),
            user: "admin".into(),
            password: "test-only".into(),
            dvrip_port: 1,
            ..CameraConfig::default()
        }
    }

    #[test]
    fn a_move_that_is_not_sent_again_stops_by_itself() {
        let ptz = Ptz::default();
        let camera = nowhere();
        let start = Instant::now();

        ptz.move_by(&camera, PtzVelocity::new(1.0, 0.0, 0.0), start)
            .unwrap();
        assert!(ptz.moving(&camera.id));

        ptz.expire(start + MOVE_LASTS / 2);
        assert!(ptz.moving(&camera.id), "not yet");

        ptz.move_by(
            &camera,
            PtzVelocity::new(1.0, 0.0, 0.0),
            start + MOVE_LASTS / 2,
        )
        .unwrap();
        ptz.expire(start + MOVE_LASTS);
        assert!(ptz.moving(&camera.id), "renewed");

        ptz.expire(start + MOVE_LASTS * 2);
        assert!(!ptz.moving(&camera.id), "it ran out and stopped");
        ptz.stop_all();
    }

    #[test]
    fn a_stop_ends_the_move_and_a_still_move_is_no_move() {
        let ptz = Ptz::default();
        let camera = nowhere();
        let now = Instant::now();

        ptz.move_by(&camera, PtzVelocity::new(0.0, 0.0, 1.0), now)
            .unwrap();
        ptz.stop(&camera.id);
        assert!(!ptz.moving(&camera.id));

        ptz.move_by(&camera, PtzVelocity::new(0.0, 0.0, 0.0), now)
            .unwrap();
        assert!(!ptz.moving(&camera.id));
        assert!(ptz.statuses().contains_key(&camera.id));

        ptz.forget(&camera.id);
        assert!(ptz.statuses().is_empty(), "the next move opens it again");
    }

    #[test]
    fn a_camera_with_no_password_is_never_tried() {
        let ptz = Ptz::default();
        let camera = CameraConfig {
            password: String::new(),
            ..nowhere()
        };

        let refused = ptz.move_by(&camera, PtzVelocity::new(1.0, 0.0, 0.0), Instant::now());

        assert_eq!(refused, Err(NO_PASSWORD.to_string()));
        assert!(ptz.statuses().is_empty(), "no connection was opened");
    }
}
