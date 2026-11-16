//! Dahua's DVRIP protocol: a binary login, then JSON calls behind a 32-byte header.
//!
//! Undocumented by Dahua. The header layouts and login hashes follow the DahuaConsole project and
//! were confirmed against an ASH21-B-V2. A client is not safe for concurrent calls.

use std::{
    fmt,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use md5::{Digest, Md5};
use serde_json::{Value, json};

const HEADER: usize = 32;
const MAX_FRAME: usize = 1024 * 1024;
const LOGIN_ACCEPTED: u16 = 0x0008;

const CHALLENGE_REQUEST: [u8; 2] = [0xa0, 0x01];
const CHALLENGE_TRAILER: [u8; 8] = [0x05, 0x02, 0x01, 0x01, 0x00, 0x00, 0xa1, 0xaa];
const LOGIN_REQUEST: [u8; 2] = [0xa0, 0x05];
const LOGIN_TRAILER: [u8; 8] = [0x05, 0x02, 0x00, 0x08, 0x00, 0x00, 0xa1, 0xaa];

/// Why a DVRIP connection or call failed.
///
/// In C# terms, one exception type per case, as an enum: the caller matches on it, and the
/// compiler makes sure the one case that must never be retried is told apart from the rest.
#[derive(Debug)]
pub enum DvripError {
    /// The camera refused the login, for the reason given. Retrying automatically could lock the
    /// account.
    LoginRejected(String),
    /// The connection could not be made, or broke.
    Io(io::Error),
    /// The camera answered with something that isn't DVRIP, or the call could not be written.
    Protocol(String),
}

impl fmt::Display for DvripError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginRejected(reason) | Self::Protocol(reason) => f.write_str(reason),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DvripError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::LoginRejected(_) | Self::Protocol(_) => None,
        }
    }
}

/// Lets `?` turn a socket's error into a [`DvripError`].
impl From<io::Error> for DvripError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a DVRIP call returns.
type Result<T> = std::result::Result<T, DvripError>;

/// A failure of the protocol itself, in words.
fn protocol(reason: impl Into<String>) -> DvripError {
    DvripError::Protocol(reason.into())
}

/// A frame's length as its header carries it: 32 bits, signed.
fn frame_length(bytes: &[u8]) -> Result<[u8; 4]> {
    i32::try_from(bytes.len())
        .map(i32::to_le_bytes)
        .map_err(|_| protocol("the call is too long to send"))
}

/// A logged-in DVRIP connection.
pub struct DvripClient {
    stream: TcpStream,
    session: u32,
    last_call: u32,
}

impl DvripClient {
    /// Opens a connection and logs in. Makes exactly one login attempt.
    pub fn connect(host: &str, port: u16, user: &str, password: &str) -> Result<Self> {
        let address = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| protocol(format!("{host}:{port} does not resolve")))?;

        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(6)))?;
        stream.set_write_timeout(Some(Duration::from_secs(6)))?;

        let mut client = Self {
            stream,
            session: 0,
            last_call: 0,
        };
        let (realm, random) = client.challenge()?;
        client.login(user, password, &realm, &random)?;
        Ok(client)
    }

    /// Sends one call and waits for the reply with the matching id.
    pub fn call(
        &mut self,
        method: &str,
        params: Option<Value>,
        object: Option<i64>,
    ) -> Result<Value> {
        self.last_call = self.last_call.wrapping_add(1);
        let call_id = self.last_call;

        let mut request = json!({
            "method": method,
            "params": params,
            "id": call_id,
            "session": self.session,
        });
        if let Some(target) = object {
            request["object"] = json!(target);
        }

        let body = serde_json::to_vec(&request).map_err(|error| protocol(error.to_string()))?;
        let length = frame_length(&body)?;
        let mut header = [0u8; HEADER];
        header[0] = 0xf6;
        header[4..8].copy_from_slice(&length);
        header[8..12].copy_from_slice(&call_id.to_le_bytes());
        header[16..20].copy_from_slice(&length);
        header[24..28].copy_from_slice(&self.session.to_le_bytes());

        self.stream.write_all(&header)?;
        self.stream.write_all(&body)?;
        self.stream.flush()?;

        self.wait_for(call_id)
    }

    fn challenge(&mut self) -> Result<(String, String)> {
        let mut header = [0u8; HEADER];
        header[..2].copy_from_slice(&CHALLENGE_REQUEST);
        header[HEADER - CHALLENGE_TRAILER.len()..].copy_from_slice(&CHALLENGE_TRAILER);
        self.stream.write_all(&header)?;
        self.stream.flush()?;

        let (_, body) = self.read_frame()?;
        let text = String::from_utf8_lossy(&body);
        let mut realm = None;
        let mut random = None;
        for line in text.split("\r\n") {
            if let Some(value) = line.strip_prefix("Realm:") {
                realm = Some(value.trim_start().to_string());
            }
            if let Some(value) = line.strip_prefix("Random:") {
                random = Some(value.trim_start().to_string());
            }
        }

        Ok((
            realm.ok_or_else(|| protocol("the login challenge has no Realm"))?,
            random.ok_or_else(|| protocol("the login challenge has no Random"))?,
        ))
    }

    fn login(&mut self, user: &str, password: &str, realm: &str, random: &str) -> Result<()> {
        let hash = login_hash(user, password, realm, random);

        let mut header = [0u8; HEADER];
        header[..2].copy_from_slice(&LOGIN_REQUEST);
        header[4..8].copy_from_slice(&frame_length(hash.as_bytes())?);
        header[HEADER - LOGIN_TRAILER.len()..].copy_from_slice(&LOGIN_TRAILER);

        self.stream.write_all(&header)?;
        self.stream.write_all(hash.as_bytes())?;
        self.stream.flush()?;

        let (reply, _) = self.read_frame()?;
        if reply.len() < 20 {
            return Err(protocol("the login reply is too short"));
        }

        let code = u16::from_be_bytes([reply[8], reply[9]]);
        if code != LOGIN_ACCEPTED {
            let tries = if reply.len() >= 30 {
                u16::from_be_bytes([reply[28], reply[29]])
            } else {
                0
            };
            return Err(DvripError::LoginRejected(describe_refusal(code, tries)));
        }

        self.session = u32::from_le_bytes([reply[16], reply[17], reply[18], reply[19]]);
        Ok(())
    }

    fn wait_for(&mut self, call_id: u32) -> Result<Value> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            let (_, chunk) = self.read_frame()?;
            buffer.extend_from_slice(&chunk);

            if let Some(reply) = parse_reply(&buffer) {
                buffer.clear();
                if reply.get("id").and_then(Value::as_u64) == Some(u64::from(call_id)) {
                    return Ok(reply);
                }
            }
        }
    }

    fn read_frame(&mut self) -> Result<([u8; HEADER], Vec<u8>)> {
        let mut header = [0u8; HEADER];
        self.stream.read_exact(&mut header)?;

        let length = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let length = usize::try_from(length)
            .ok()
            .filter(|length| *length <= MAX_FRAME)
            .ok_or_else(|| protocol("that port does not speak DVRIP"))?;

        let mut body = vec![0u8; length];
        self.stream.read_exact(&mut body)?;
        Ok((header, body))
    }
}

/// The login text: both password hashes the camera may check.
pub fn login_hash(user: &str, password: &str, realm: &str, random: &str) -> String {
    let digest = md5_hex(&format!("{user}:{realm}:{password}"));
    let modern = md5_hex(&format!("{user}:{random}:{digest}"));
    let legacy = md5_hex(&format!("{user}:{random}:{}", legacy_hash(password)));
    format!("{user}&&{modern}{legacy}")
}

/// Dahua's old 8-character password hash.
pub fn legacy_hash(password: &str) -> String {
    let bytes = password.as_bytes();
    let digest = Md5::digest(bytes);
    let mut result = String::with_capacity(digest.len() / 2);
    for pair in digest.as_chunks::<2>().0 {
        // At most 61, so it fits a byte.
        let value = ((u16::from(pair[0]) + u16::from(pair[1])) % 62) as u8;
        result.push(match value {
            0..=9 => (value + b'0') as char,
            10..=35 => (value + b'A' - 10) as char,
            _ => (value + b'a' - 36) as char,
        });
    }
    result
}

/// An MD5 digest in uppercase hex, the form the login hashes take.
fn md5_hex(text: &str) -> String {
    let digest = Md5::digest(text.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// A reply can be JSON, or JSON followed by a newline and binary data.
fn parse_reply(buffer: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(buffer);
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        return Some(value);
    }
    let first = text.split('\n').next()?;
    serde_json::from_str::<Value>(first).ok()
}

fn describe_refusal(code: u16, tries: u16) -> String {
    match code {
        0x0100 => format!("wrong username or password, {tries} tries left before lockout"),
        0x0101 => "username not recognized".to_string(),
        0x0104 => "the account is locked".to_string(),
        0x0111 => "the camera has not been set up yet".to_string(),
        0x0303 => "that user is already logged in".to_string(),
        _ => format!("login refused with code {code:04X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_hash_matches_the_known_shape() {
        // The legacy hash is always eight characters from [0-9A-Za-z].
        let hash = legacy_hash("admin");
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn login_hash_has_both_suffixes() {
        let hash = login_hash("admin", "secret", "LoginToMoreWide", "12345");
        assert!(hash.starts_with("admin&&"));
        // user&& + two 32-character MD5 hex digests: the modern hash, then the legacy one.
        assert_eq!(hash.len(), "admin&&".len() + 32 + 32);
    }

    #[test]
    fn parses_a_reply_with_trailing_binary() {
        let mut buffer = br#"{"id":7,"result":true}"#.to_vec();
        buffer.extend_from_slice(b"\n\x00\x01\x02");
        let parsed = parse_reply(&buffer).expect("should parse the JSON prefix");
        assert_eq!(parsed["id"], 7);
    }
}
