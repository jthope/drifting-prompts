//! PTZ for the web: one DVRIP connection per camera, held by the server, and moves that stop by
//! themselves.
//!
//! The desktop moves a camera while a button is held and stops it when the button is let go. Over
//! the network the stop can be lost (the tab closes, the connection drops mid-move), and a lost
//! stop leaves the camera turning. So on the web a move lasts [`MOVE_LASTS`] unless the web build
//! sends it again, which it does while the button is held, and the server stops any move that runs
//! out ([`Ptz::expire`]).
//!
//! The connection is `core`'s [`PtzSession`], opened on a camera's first move as the desktop does,
//! and closed (stopping the camera) when the camera's settings change, here or in the files by
//! another program, it is deleted, or the server stops, which waits for those stops to go out
//! (`closing.rs`). A camera with no saved password is never tried: a blank login fails, and failed
//! logins can lock the camera's account.
//!
//! A stop never waits behind the moves sent before it: `PtzSession` sends the camera the newest
//! command only, so a held move's repeats don't pile up ahead of the stop that ends it.

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

/// How long a move lasts unless it is sent again. The web build sends a held move about every
/// 400 ms, so this rides out a late request, and a lost stop costs at most this much turning.
pub(crate) const MOVE_LASTS: Duration = Duration::from_secs(1);

/// One camera's PTZ connection, and the settings it logged in with.
struct Held {
    setting: CameraConfig,
    /// Closed off the runtime's threads when this is dropped: closing sends the camera a stop and
    /// waits for PTZ's thread.
    session: OffThread<PtzSession>,
    /// When the move under way runs out, while one is.
    moving_until: Option<Instant>,
}

impl Held {
    fn session(&self) -> &PtzSession {
        &self.session
    }
}

/// Every camera's PTZ connection that has been opened.
#[derive(Default)]
pub(crate) struct Ptz {
    held: Mutex<HashMap<CameraId, Held>>,
    /// The connections that are open or still closing, for the server to wait for as it ends.
    closing: Arc<Closing>,
}

impl Ptz {
    /// Starts or changes a move of `camera` at `velocity`, which lasts [`MOVE_LASTS`] from `now`
    /// unless sent again. The first move opens the connection; settings that changed since open
    /// it anew.
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

    /// Stops `id`'s move, if it has a connection.
    pub(crate) fn stop(&self, id: &CameraId) {
        if let Some(open) = self.held.locked().get_mut(id) {
            open.session().stop();
            open.moving_until = None;
        }
    }

    /// Stops every move that has run out by `now`: its stop was lost, or the page that held it
    /// went away.
    pub(crate) fn expire(&self, now: Instant) {
        for open in self.held.locked().values_mut() {
            if open.moving_until.is_some_and(|until| until <= now) {
                open.session().stop();
                open.moving_until = None;
            }
        }
    }

    /// What each camera's PTZ last reported, for those with a connection.
    pub(crate) fn statuses(&self) -> HashMap<CameraId, String> {
        self.held
            .locked()
            .iter()
            .map(|(id, open)| (id.clone(), open.session().status()))
            .collect()
    }

    /// Closes `id`'s connection, stopping it, after its settings changed or it was deleted. The
    /// next move opens it again.
    pub(crate) fn forget(&self, id: &CameraId) {
        self.held.locked().remove(id);
    }

    /// Closes the connection of every camera whose settings are no longer what it logged in with,
    /// or that is gone: what [`forget`](Self::forget) does for a change made here, for one another
    /// program made to the files.
    pub(crate) fn follow(&self, library: &Library) {
        self.held
            .locked()
            .retain(|id, open| library.camera(id).as_ref() == Some(&open.setting));
    }

    /// Closes every connection, stopping every camera, as the server ends. Each closes on a thread
    /// of its own: [`stopped_by`](Self::stopped_by) waits for them.
    pub(crate) fn stop_all(&self) {
        self.held.locked().clear();
    }

    /// Waits until every connection has closed, its stop sent, or `until`. Returns whether they
    /// all have. It blocks, so it runs off the runtime's threads.
    pub(crate) fn stopped_by(&self, until: Instant) -> bool {
        self.closing.wait(until)
    }

    /// Whether `id` is moving: a move was sent and hasn't run out or been stopped.
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

    /// A camera whose PTZ port nothing listens on, so its connection fails at once rather than
    /// reaching a real camera.
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

        // Sent again, it runs on from then.
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
