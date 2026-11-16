//! Windows' Data Protection API (DPAPI) behind two safe calls: [`protect`] encrypts bytes so that
//! only the Windows account that encrypted them, on the same machine, can decrypt them, and
//! [`unprotect`] decrypts them.
//!
//! The key is Windows' own: it comes from the account's logon credentials and Windows keeps it,
//! never this crate or a file it writes. In C# terms, [`protect`] is
//! `ProtectedData.Protect(data, null, DataProtectionScope.CurrentUser)`, which makes the same call,
//! `CryptProtectData`.
//!
//! That keeps the encrypted bytes from being read anywhere else: on another machine, or by another
//! account on this one, whether they got there through a backup, a synced folder or a file attached
//! to a bug report. It doesn't keep them from a program running as the same account, which can call
//! [`unprotect`] as this crate does.
//!
//! Neither call shows anything (`CRYPTPROTECT_UI_FORBIDDEN`), so both work in a program with no
//! window, such as a server. This crate is the root workspace's only `unsafe`: the two calls,
//! reading what they return, and freeing it. On every other system it is empty.
#![cfg(windows)]

use std::{fmt, io, ptr, slice};

use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
};

/// Why DPAPI could not encrypt or decrypt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DpapiError {
    /// The data is longer than DPAPI takes in one call, 4 GiB.
    TooLong,
    /// Windows could not encrypt the data. Its error code.
    Protect(i32),
    /// Windows could not decrypt the data: another account or another machine encrypted it, it was
    /// changed since, or it was never DPAPI's. Its error code.
    Unprotect(i32),
}

impl fmt::Display for DpapiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => f.write_str("the data is too long to encrypt in one piece"),
            Self::Protect(code) => write!(
                f,
                "Windows could not encrypt the data ({})",
                io::Error::from_raw_os_error(*code)
            ),
            Self::Unprotect(code) => write!(
                f,
                "Windows could not decrypt the data ({})",
                io::Error::from_raw_os_error(*code)
            ),
        }
    }
}

impl std::error::Error for DpapiError {}

/// `data` encrypted for the Windows account running this program, on this machine. Each call
/// gives different bytes, even for the same data.
pub fn protect(data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    let input = borrowed(data)?;
    let mut output = empty();
    // SAFETY: `input` points at `data`'s `cbData` bytes, which outlive the call and which DPAPI only
    // reads. The description, entropy, reserved and prompt arguments may all be null. `output` is
    // for DPAPI to point at a buffer of its own, which `take` frees.
    let done = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if done == 0 {
        return Err(DpapiError::Protect(last_error()));
    }
    Ok(take(output, false))
}

/// The data that [`protect`] encrypted into `data`, when this account on this machine encrypted it.
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    let input = borrowed(data)?;
    let mut output = empty();
    // SAFETY: as in `protect`. The description's out-pointer is null too, so DPAPI allocates none.
    let done = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if done == 0 {
        return Err(DpapiError::Unprotect(last_error()));
    }
    Ok(take(output, true))
}

/// `data` as DPAPI takes its input.
fn borrowed(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, DpapiError> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| DpapiError::TooLong)?,
        // DPAPI reads its input through a `*const` blob and never writes to it: the field is
        // `*mut` only because the same struct carries the output.
        pbData: data.as_ptr().cast_mut(),
    })
}

/// A blob for DPAPI to fill.
fn empty() -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    }
}

/// Copies what DPAPI returned in `blob`, then frees DPAPI's buffer. A buffer holding decrypted data
/// is overwritten first (`wipe`), so no copy of the secret is left behind in freed memory.
fn take(blob: CRYPT_INTEGER_BLOB, wipe: bool) -> Vec<u8> {
    if blob.pbData.is_null() {
        return Vec::new();
    }
    let len = blob.cbData as usize;
    // SAFETY: after a call that succeeded, `pbData` points at `cbData` bytes that DPAPI allocated
    // for this call alone.
    let bytes = unsafe { slice::from_raw_parts(blob.pbData, len) }.to_vec();
    if wipe {
        // SAFETY: the same `len` bytes, still DPAPI's buffer and not yet freed.
        unsafe { ptr::write_bytes(blob.pbData, 0, len) };
    }
    // SAFETY: DPAPI allocates its output with `LocalAlloc`, which `LocalFree` releases. It is freed
    // once, here, and not used after.
    unsafe { LocalFree(blob.pbData.cast()) };
    bytes
}

/// The error code of the Win32 call that just failed on this thread.
fn last_error() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"correct horse battery staple";

    fn holds(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|part| part == needle)
    }

    #[test]
    fn protected_data_unprotects_to_itself_and_is_not_in_the_blob() {
        let sealed = protect(SECRET).unwrap();

        assert!(
            !holds(&sealed, SECRET),
            "the data is not in the blob as itself"
        );
        assert_eq!(unprotect(&sealed).unwrap(), SECRET);
    }

    #[test]
    fn the_same_data_protects_differently_each_time() {
        let first = protect(SECRET).unwrap();
        let second = protect(SECRET).unwrap();

        assert_ne!(first, second);
        assert_eq!(unprotect(&first).unwrap(), unprotect(&second).unwrap());
    }

    #[test]
    fn nothing_protects_and_unprotects_too() {
        let sealed = protect(b"").unwrap();

        assert!(unprotect(&sealed).unwrap().is_empty());
    }

    #[test]
    fn data_that_dpapi_never_protected_does_not_unprotect() {
        assert!(matches!(
            unprotect(SECRET),
            Err(DpapiError::Unprotect(code)) if code != 0
        ));
        assert!(matches!(unprotect(b""), Err(DpapiError::Unprotect(_))));
    }

    #[test]
    fn a_changed_blob_does_not_unprotect() {
        let mut sealed = protect(SECRET).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;

        let error = unprotect(&sealed).unwrap_err();

        assert!(matches!(error, DpapiError::Unprotect(_)));
        assert!(
            error
                .to_string()
                .starts_with("Windows could not decrypt the data ("),
            "{error}"
        );
    }
}
