//! A dropped folder's `file:` URL as a path, a host naming a network folder.

use std::path::PathBuf;

/// The path `uri` names, or `None` for what isn't a `file:` URL.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8_lossy()
        .into_owned();
    let (host, path) = match decoded.find('/') {
        Some(at) => decoded.split_at(at),
        None => (decoded.as_str(), ""),
    };
    let path = if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        // `/D:/Notes` drops the slash before the drive letter.
        let bytes = path.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            path[1..].to_owned()
        } else {
            path.to_owned()
        }
    } else {
        format!("//{host}{path}")
    };
    if path.is_empty() {
        return None;
    }
    let native = if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path
    };
    Some(PathBuf::from(native))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(path: &str) -> PathBuf {
        PathBuf::from(if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.to_owned()
        })
    }

    #[test]
    fn a_drives_folder() {
        assert_eq!(
            path_of("file:///D:/Notes/My%20Notes"),
            Some(native("D:/Notes/My Notes"))
        );
    }

    #[test]
    fn letters_beyond_ascii() {
        assert_eq!(
            path_of("file:///C:/Users/Ren%C3%A9e/Notizen"),
            Some(native("C:/Users/Renée/Notizen"))
        );
    }

    #[test]
    fn a_network_folder() {
        assert_eq!(
            path_of("file://server/share/notes"),
            Some(native("//server/share/notes"))
        );
    }

    #[test]
    fn a_path_from_root() {
        assert_eq!(
            path_of("file:///home/me/notes"),
            Some(native("/home/me/notes"))
        );
    }

    #[test]
    fn what_isnt_a_file_url() {
        assert_eq!(path_of("https://example.com/notes"), None);
        assert_eq!(path_of("file://"), None);
    }

    #[test]
    fn a_stray_percent_stays() {
        assert_eq!(
            path_of("file:///D:/100%25%20done/50%"),
            Some(native("D:/100% done/50%"))
        );
    }
}
