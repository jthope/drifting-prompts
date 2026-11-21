//! The page the rendered view shows a file on: its HTML, style and script (`page/`), built into
//! the app.
//!
//! The web view loads it once; each file, theme and scroll after that is a message
//! (`views/page.tsx`). Its Content-Security-Policy runs only the page's own style and script, by a
//! nonce made each time the page is handed out, and loads pictures only as `data:` URLs or over
//! HTTPS, so nothing a file holds runs or reaches anything else.

use std::{
    hash::{BuildHasher, RandomState},
    time::{SystemTime, UNIX_EPOCH},
};

const HTML: &str = include_str!("../page/page.html");
const STYLE: &str = include_str!("../page/page.css");
const SCRIPT: &str = include_str!("../page/page.js");

/// The page's HTML with its style, script and a new nonce in it.
pub(crate) fn shell() -> String {
    HTML.replace("{nonce}", &nonce())
        .replace("{style}", STYLE)
        .replace("{script}", SCRIPT)
}

/// 128 bits no file can guess: the standard library's hasher keys are random for each process.
fn nonce() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let state = RandomState::new();
    format!("{:016x}{:016x}", state.hash_one(now), state.hash_one(!now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_carries_its_style_and_script_under_a_new_nonce() {
        let one = shell();
        let nonce = one
            .split("'nonce-")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("a nonce");
        assert_eq!(nonce.len(), 32);
        assert!(one.contains(&format!("<style nonce=\"{nonce}\">")));
        assert!(one.contains(&format!("<script nonce=\"{nonce}\">")));
        assert!(!one.contains("{style}") && !one.contains("{script}"));
        assert!(!shell().contains(nonce));
    }
}
