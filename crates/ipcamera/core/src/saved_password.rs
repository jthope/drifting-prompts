//! How `cameras.json` keeps a camera's password.
//!
//! The app sends each camera its password (DVRIP's login, Reolink's, and the digests of ONVIF's
//! and RTSP's are all made from it), so it can't be kept as a hash the way the server's own
//! password is. On Windows the file keeps it encrypted by DPAPI for the Windows account running the
//! app (`ipcamera-dpapi`), as Base64. Wherever else the file goes (another machine, another
//! account's reach, a backup, a bug report), it gives no password away. A program running as the
//! same account can still decrypt it, as the app does.
//!
//! A value this account can't decrypt (one encrypted by another account, or on another machine)
//! reads as no password: the camera waits for one to be entered, and nothing logs in without one.
//! The next save of the file writes none in its place.
//!
//! On other systems (the server on Linux) the file keeps the password as it is, and `dp-store`
//! writes it readable by its owner only ([`Document::OWNER_ONLY`](dp_store::Document::OWNER_ONLY)).
//! The web build keeps its cameras in memory and writes no file.
//!
//! No password is an empty value either way, so the file shows which cameras have one.

#[cfg(windows)]
use base64::{Engine, engine::general_purpose::STANDARD};

/// `password` as `cameras.json` keeps it: encrypted for this Windows account, as Base64.
#[cfg(windows)]
pub(crate) fn seal(password: &str) -> Result<String, ipcamera_dpapi::DpapiError> {
    if password.is_empty() {
        return Ok(String::new());
    }
    ipcamera_dpapi::protect(password.as_bytes()).map(|sealed| STANDARD.encode(sealed))
}

/// The password `saved` holds: empty when there is none, or when this account can't decrypt it.
#[cfg(windows)]
pub(crate) fn open(saved: &str) -> String {
    if saved.is_empty() {
        return String::new();
    }
    STANDARD
        .decode(saved)
        .ok()
        .and_then(|sealed| ipcamera_dpapi::unprotect(&sealed).ok())
        .and_then(|password| String::from_utf8(password).ok())
        .unwrap_or_default()
}

/// `password` as `cameras.json` keeps it: as it is, in a file only its owner can read.
#[cfg(not(windows))]
pub(crate) fn seal(password: &str) -> Result<String, std::convert::Infallible> {
    Ok(password.to_owned())
}

/// The password `saved` holds.
#[cfg(not(windows))]
pub(crate) fn open(saved: &str) -> String {
    saved.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_opens_to_itself() {
        let saved = seal("s3cret").unwrap();

        assert_eq!(open(&saved), "s3cret");
    }

    #[test]
    fn no_password_is_saved_as_none() {
        assert_eq!(seal("").unwrap(), "");
        assert_eq!(open(""), "");
    }

    #[cfg(windows)]
    #[test]
    fn a_password_is_never_saved_as_itself_or_the_same_way_twice() {
        let first = seal("s3cret").unwrap();
        let second = seal("s3cret").unwrap();

        assert!(!first.contains("s3cret"), "{first}");
        assert_ne!(first, second, "DPAPI salts each one");
        assert_eq!(open(&second), "s3cret");
    }

    #[cfg(windows)]
    #[test]
    fn a_value_this_account_cannot_decrypt_is_no_password() {
        // Not Base64 at all, then Base64 of bytes DPAPI never encrypted, then one it did, changed.
        let mut changed = STANDARD.decode(seal("s3cret").unwrap()).unwrap();
        let last = changed.len() - 1;
        changed[last] ^= 0xFF;
        for saved in [
            "s3cret".to_owned(),
            STANDARD.encode("s3cret"),
            STANDARD.encode(changed),
        ] {
            assert_eq!(open(&saved), "", "{saved}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_password_is_saved_as_it_is() {
        assert_eq!(seal("s3cret").unwrap(), "s3cret");
    }
}
