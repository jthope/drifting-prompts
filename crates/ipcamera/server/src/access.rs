//! Who may use the server: its password, and the sessions of those who gave it.
//!
//! A server with no password asks for none, and answers this machine as before. One with a
//! password (`ipcamera-server --set-password`) answers nothing but its pages' own files and
//! `/api/session` until a request comes with a session's key:
//!
//! - `GET /api/session` says whether a password is needed and whether the request's key is good.
//! - `POST /api/session` takes the password and answers with a new session's key.
//! - `DELETE /api/session` ends the request's session.
//!
//! A key goes with a request as `Authorization: Bearer <key>`, or as `?key=<key>` where the
//! browser makes the request itself and no header can be set: a tile's player, its WebSocket, a
//! picture, a file opened in a tab. Keys in addresses are why every answer asks the browser to
//! send no `Referer`.
//!
//! The password is kept in `settings.json` as a salted Argon2 hash (`server`), never as itself,
//! and is read when the server starts. A session is a random key held in memory: it ends when it
//! hasn't been used for [`SESSION_LASTS`], when it is signed out, or with the server. After a
//! wrong password the next try waits [`WAIT_AFTER_WRONG`], which makes guessing slow.
//!
//! This keeps other programs and users on this machine out. It is not yet enough to listen on the
//! network with: the password and the keys would cross it in the clear, which needs HTTPS first.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{Salt, SaltString},
};
use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipcamera_core::{
    api::{Session, SignIn, SignedIn},
    sync::LockExt,
};

use crate::Server;

/// How long a session lasts after its last request.
const SESSION_LASTS: Duration = Duration::from_secs(12 * 60 * 60);

/// How long the next try waits after a wrong password.
const WAIT_AFTER_WRONG: Duration = Duration::from_secs(2);

/// What a request with no session gets from a server that has a password.
const SIGN_IN_FIRST: &str = "Sign in to the server first.";

/// Why a password can't be the server's.
const NO_PASSWORD_GIVEN: &str = "The password can't be empty.";

/// Why signing in failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The password was wrong.
    Wrong,
    /// A wrong password came a moment ago.
    TooSoon,
}

/// The hash to save for `password`, in the form Argon2's own tools write (PHC), salt included.
///
/// For `--set-password`. The error is a sentence for the terminal.
pub fn hash_password(password: &str) -> Result<String, String> {
    if password.is_empty() {
        return Err(NO_PASSWORD_GIVEN.to_string());
    }
    // A salt only has to differ from password to password: a random id's bytes do.
    let salt = uuid::Uuid::new_v4();
    let salt = SaltString::encode_b64(&salt.as_bytes()[..Salt::RECOMMENDED_LENGTH])
        .map_err(|error| format!("The password couldn't be hashed ({error})."))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("The password couldn't be hashed ({error})."))
}

/// Whether `password` is the one `saved` (a hash from [`hash_password`]) was made from. A saved
/// value that isn't such a hash matches nothing.
fn matches(saved: &str, password: &str) -> bool {
    PasswordHash::new(saved).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

/// The server's password and its sessions.
pub(crate) struct Access {
    /// The saved hash; empty for a server with no password.
    password: String,
    /// Each session's key, and when it was last used.
    sessions: Mutex<HashMap<String, Instant>>,
    /// Until when no password is tried, after a wrong one.
    wait_until: Mutex<Option<Instant>>,
}

impl Access {
    /// Access by the password whose hash is `password`; none is needed when it is empty.
    pub(crate) fn new(password: String) -> Self {
        Self {
            password,
            sessions: Mutex::default(),
            wait_until: Mutex::new(None),
        }
    }

    /// Whether the server has a password.
    pub(crate) fn required(&self) -> bool {
        !self.password.is_empty()
    }

    /// Starts a session for whoever gave `password`, returning its key. Hashing takes a moment,
    /// so this runs off the runtime's threads.
    pub(crate) fn sign_in(&self, password: &str, now: Instant) -> Result<String, Refused> {
        {
            let mut wait_until = self.wait_until.locked();
            if wait_until.is_some_and(|until| now < until) {
                return Err(Refused::TooSoon);
            }
            // Held from here, so tries made together wait their turn like any others.
            *wait_until = Some(now + WAIT_AFTER_WRONG);
        }
        if !self.required() || !matches(&self.password, password) {
            return Err(Refused::Wrong);
        }
        *self.wait_until.locked() = None;

        // Two random ids: 244 random bits, from the system's generator.
        let key = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut sessions = self.sessions.locked();
        sessions.retain(|_, used| now.duration_since(*used) < SESSION_LASTS);
        sessions.insert(key.clone(), now);
        Ok(key)
    }

    /// Whether a request with `key` may be answered: no password is needed, or the key is a
    /// session's, which this counts as used.
    pub(crate) fn allows(&self, key: Option<&str>, now: Instant) -> bool {
        if !self.required() {
            return true;
        }
        let Some(key) = key else {
            return false;
        };
        let mut sessions = self.sessions.locked();
        match sessions.get_mut(key) {
            Some(used) if now.duration_since(*used) < SESSION_LASTS => {
                *used = now;
                true
            }
            Some(_) => {
                sessions.remove(key);
                false
            }
            None => false,
        }
    }

    /// Ends the session with `key`.
    pub(crate) fn sign_out(&self, key: &str) {
        self.sessions.locked().remove(key);
    }
}

/// The session key `request` came with: `Authorization: Bearer <key>`, or `?key=<key>`.
fn key_of(request: &Request) -> Option<String> {
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);
    bearer.or_else(|| {
        request
            .uri()
            .query()?
            .split('&')
            .find_map(|pair| pair.strip_prefix("key="))
            .map(str::to_string)
    })
}

/// Turns away a request with no session, when the server has a password.
pub(crate) async fn signed_in_only(
    State(server): State<Server>,
    request: Request,
    next: Next,
) -> Response {
    if server
        .access
        .allows(key_of(&request).as_deref(), Instant::now())
    {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, SIGN_IN_FIRST).into_response()
    }
}

/// `GET /api/session`: whether a password is needed, and whether the request's key is good.
pub(crate) async fn session(State(server): State<Server>, request: Request) -> Json<Session> {
    Json(Session {
        required: server.access.required(),
        signed_in: server
            .access
            .allows(key_of(&request).as_deref(), Instant::now()),
    })
}

/// `POST /api/session`: a new session for the right password.
pub(crate) async fn sign_in(State(server): State<Server>, Json(given): Json<SignIn>) -> Response {
    if !server.access.required() {
        return (
            StatusCode::BAD_REQUEST,
            "This server has no password, so there is nothing to sign in to.",
        )
            .into_response();
    }
    let access = server.access.clone();
    let signed_in =
        tokio::task::spawn_blocking(move || access.sign_in(&given.password, Instant::now())).await;
    match signed_in {
        Ok(Ok(key)) => Json(SignedIn { key }).into_response(),
        Ok(Err(Refused::Wrong)) => (
            StatusCode::UNAUTHORIZED,
            "That isn't the server's password.",
        )
            .into_response(),
        Ok(Err(Refused::TooSoon)) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Wait a few seconds, then try the password again.",
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The server is stopping. Try again once it is back.",
        )
            .into_response(),
    }
}

/// `DELETE /api/session`: ends the request's session.
pub(crate) async fn sign_out(State(server): State<Server>, request: Request) -> StatusCode {
    if let Some(key) = key_of(&request) {
        server.access.sign_out(&key);
    }
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A password of the tests' own.
    const PASSWORD: &str = "test-only";

    fn access() -> Access {
        Access::new(hash_password(PASSWORD).unwrap())
    }

    #[test]
    fn a_server_with_no_password_asks_for_none() {
        let access = Access::new(String::new());
        assert!(!access.required());
        assert!(access.allows(None, Instant::now()));
        assert_eq!(
            access.sign_in("", Instant::now()),
            Err(Refused::Wrong),
            "and there is nothing to sign in to"
        );
    }

    #[test]
    fn the_password_is_saved_as_a_hash_that_differs_each_time() {
        let (first, second) = (
            hash_password(PASSWORD).unwrap(),
            hash_password(PASSWORD).unwrap(),
        );
        assert!(first.starts_with("$argon2id$"), "{first}");
        assert!(!first.contains(PASSWORD));
        assert_ne!(first, second, "each has a salt of its own");
        assert!(matches(&first, PASSWORD) && matches(&second, PASSWORD));
        assert!(!matches(&first, "something else"));
        assert!(!matches("not a hash", "not a hash"));
        assert_eq!(hash_password(""), Err(NO_PASSWORD_GIVEN.to_string()));
    }

    #[test]
    fn only_the_right_password_starts_a_session() {
        let access = access();
        let now = Instant::now();
        assert!(access.required());
        assert!(!access.allows(None, now));
        assert!(!access.allows(Some("made-up"), now));

        let key = access.sign_in(PASSWORD, now).unwrap();
        assert_eq!(key.len(), 64);
        assert!(access.allows(Some(&key), now));
        let other = access.sign_in(PASSWORD, now).unwrap();
        assert_ne!(key, other, "each session has a key of its own");

        access.sign_out(&key);
        assert!(!access.allows(Some(&key), now));
        assert!(access.allows(Some(&other), now), "the other carries on");
    }

    #[test]
    fn a_wrong_password_makes_the_next_try_wait() {
        let access = access();
        let now = Instant::now();
        assert_eq!(access.sign_in("guess", now), Err(Refused::Wrong));
        assert_eq!(
            access.sign_in(PASSWORD, now + WAIT_AFTER_WRONG / 2),
            Err(Refused::TooSoon),
            "even the right one"
        );
        assert!(access.sign_in(PASSWORD, now + WAIT_AFTER_WRONG).is_ok());
    }

    #[test]
    fn a_session_ends_when_it_goes_unused() {
        let access = access();
        let now = Instant::now();
        let key = access.sign_in(PASSWORD, now).unwrap();

        let later = now + SESSION_LASTS - Duration::from_secs(1);
        assert!(access.allows(Some(&key), later), "used, so it runs on");
        assert!(access.allows(Some(&key), later + SESSION_LASTS / 2));
        assert!(!access.allows(Some(&key), later + SESSION_LASTS * 2));
    }
}
