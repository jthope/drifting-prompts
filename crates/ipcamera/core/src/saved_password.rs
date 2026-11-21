//! How `cameras.json` keeps a camera's password: encrypted by DPAPI for this account, as Base64, or
//! without DPAPI as it is, in a file only its owner can read. A value this account can't decrypt
//! reads as no password, so nothing logs in with it.

#[cfg(windows)]
use base64::{Engine, engine::general_purpose::STANDARD};

#[cfg(windows)]
pub(crate) fn seal(password: &str) -> Result<String, ipcamera_dpapi::DpapiError> {
    if password.is_empty() {
        return Ok(String::new());
    }
    ipcamera_dpapi::protect(password.as_bytes()).map(|sealed| STANDARD.encode(sealed))
}

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

#[cfg(not(windows))]
pub(crate) fn seal(password: &str) -> Result<String, std::convert::Infallible> {
    Ok(password.to_owned())
}

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
