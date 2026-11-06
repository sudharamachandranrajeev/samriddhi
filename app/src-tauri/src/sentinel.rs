//! Optional Windows integration with Sentinel (the Brahma security service).
//! Samriddhi never needs it: absent, unreachable or incompatible is just a
//! status row, and nothing else changes. Sentinel's protocol does not expose
//! Defender or tamper health, so "running" is all this ever claims.
//!
//! Wire format (Sentinel's `sentinel-core/src/protocol.rs`): each frame is a
//! little-endian u32 byte length followed by that many bytes of UTF-8 JSON;
//! one request gets one response. We send `{"type":"hello","protocol":1}` and
//! expect `{"type":"hello","protocol":N,"service_version":"x"}` on the pipe
//! `\\.\pipe\sentinel`.
//!
//! Only opening the pipe is Windows code. Framing, the reply reading and the
//! exchange over any `Read + Write` are plain and tested everywhere.

use std::io::{Read, Write};

use serde::Serialize;
use serde_json::{Value, json};

#[cfg(any(windows, test))]
pub const PIPE: &str = r"\\.\pipe\sentinel";
pub const PROTOCOL: u64 = 1;
/// The most a frame may carry, either way.
pub const MAX_FRAME: usize = 1024 * 1024;
const MAX_VERSION: usize = 40;

/// What the Sentinel row can say.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SentinelStatus {
    /// Not Windows, or Sentinel is not installed or not running.
    Absent,
    /// Answered the hello with a protocol we speak.
    Running { version: String },
    /// There, but its pipe is for administrators only (its default).
    Restricted,
    /// Answered, but in a way this version does not understand.
    Incompatible { detail: String },
    /// No usable answer.
    Error { detail: String },
}

fn error(detail: impl Into<String>) -> SentinelStatus {
    SentinelStatus::Error {
        detail: detail.into(),
    }
}

/// A message as one frame: u32 little-endian length, then the JSON.
pub fn encode_frame(message: &Value) -> Result<Vec<u8>, String> {
    let body = serde_json::to_vec(message).map_err(|_| "could not write the message")?;
    if body.len() > MAX_FRAME {
        return Err("frame too large".into());
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// The body length a frame header announces. A header that claims more than
/// [`MAX_FRAME`], or is short, is refused.
pub fn frame_length(header: &[u8]) -> Result<usize, String> {
    let Some(h) = header.first_chunk::<4>() else {
        return Err("short header".into());
    };
    let n = u32::from_le_bytes(*h) as usize;
    if n > MAX_FRAME {
        return Err("frame too large".into());
    }
    Ok(n)
}

/// Sentinel's reply to our hello, as a status. The version is shown to the
/// user, so it is cut to a short, plain string.
pub fn interpret_hello(reply: &Value) -> SentinelStatus {
    if reply["type"] != "hello" {
        return SentinelStatus::Incompatible {
            detail: "unexpected reply".into(),
        };
    }
    if reply["protocol"] != json!(PROTOCOL) {
        let p = reply["protocol"]
            .as_u64()
            .map_or("unknown".to_string(), |p| p.to_string());
        return SentinelStatus::Incompatible {
            detail: format!("protocol {p}"),
        };
    }
    let version: String = reply["service_version"]
        .as_str()
        .unwrap_or("unknown")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_VERSION)
        .collect();
    SentinelStatus::Running {
        version: if version.is_empty() {
            "unknown".into()
        } else {
            version
        },
    }
}

/// Send the hello and read the reply over `io`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub fn hello<T: Read + Write>(io: &mut T) -> SentinelStatus {
    let frame = match encode_frame(&json!({ "type": "hello", "protocol": PROTOCOL })) {
        Ok(f) => f,
        Err(e) => return error(e),
    };
    if io.write_all(&frame).and_then(|()| io.flush()).is_err() {
        return error("could not send the hello");
    }
    let mut header = [0u8; 4];
    if io.read_exact(&mut header).is_err() {
        return error("no answer");
    }
    let len = match frame_length(&header) {
        Ok(n) => n,
        Err(e) => return SentinelStatus::Incompatible { detail: e },
    };
    let mut body = vec![0u8; len];
    if io.read_exact(&mut body).is_err() {
        return error("the answer was cut short");
    }
    match serde_json::from_slice::<Value>(&body) {
        Ok(v) => interpret_hello(&v),
        Err(_) => SentinelStatus::Incompatible {
            detail: "the answer was not JSON".into(),
        },
    }
}

/// Ask Sentinel for its hello. Windows only; anywhere else, absent. Blocks,
/// so callers run it off the main thread; it gives up after a short while.
pub fn query() -> SentinelStatus {
    #[cfg(windows)]
    {
        windows::query()
    }
    #[cfg(not(windows))]
    {
        SentinelStatus::Absent
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(2);
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_ACCESS_DENIED: i32 = 5;

    pub fn query() -> SentinelStatus {
        // A pipe read has no timeout of its own: do it on a thread and stop
        // waiting for it after TIMEOUT (the thread ends when the pipe does).
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(talk());
        });
        rx.recv_timeout(TIMEOUT)
            .unwrap_or_else(|_| super::error("no answer in time"))
    }

    fn talk() -> SentinelStatus {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(PIPE);
        match opened {
            Ok(mut pipe) => hello(&mut pipe),
            Err(e) => match e.raw_os_error() {
                Some(ERROR_FILE_NOT_FOUND) => SentinelStatus::Absent,
                Some(ERROR_ACCESS_DENIED) => SentinelStatus::Restricted,
                other => super::error(format!("open failed ({other:?})")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A pipe stand-in: reads come from `reply`, writes are kept.
    struct Fake {
        reply: Cursor<Vec<u8>>,
        sent: Vec<u8>,
    }
    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reply.read(buf)
        }
    }
    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.sent.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn fake(reply: Vec<u8>) -> Fake {
        Fake {
            reply: Cursor::new(reply),
            sent: Vec::new(),
        }
    }

    #[test]
    fn hello_frame_matches_sentinel_wire_format() {
        let f = encode_frame(&json!({ "type": "hello", "protocol": 1 })).unwrap();
        let text = r#"{"type":"hello","protocol":1}"#;
        assert_eq!(f[..4], [text.len() as u8, 0, 0, 0]);
        assert_eq!(std::str::from_utf8(&f[4..]).unwrap(), text);
        assert_eq!(frame_length(&f[..4]), Ok(text.len()));
    }

    #[test]
    fn oversized_or_short_headers_are_refused() {
        let big = ((MAX_FRAME + 1) as u32).to_le_bytes();
        assert!(frame_length(&big).is_err());
        assert_eq!(
            frame_length(&(MAX_FRAME as u32).to_le_bytes()),
            Ok(MAX_FRAME)
        );
        assert!(frame_length(&[1, 2]).is_err());
        let huge = json!({ "x": "a".repeat(MAX_FRAME) });
        assert!(encode_frame(&huge).is_err());
    }

    #[test]
    fn interpret_hello_reads_the_reply() {
        assert_eq!(
            interpret_hello(&json!({ "type": "hello", "protocol": 1, "service_version": "0.3" })),
            SentinelStatus::Running {
                version: "0.3".into()
            }
        );
        assert!(matches!(
            interpret_hello(&json!({ "type": "hello", "protocol": 2 })),
            SentinelStatus::Incompatible { detail } if detail == "protocol 2"
        ));
        assert!(matches!(
            interpret_hello(&json!({ "type": "error" })),
            SentinelStatus::Incompatible { .. }
        ));
        // A version is shown to the user: plain and short.
        let long = format!("1.0\n{}", "x".repeat(200));
        let SentinelStatus::Running { version } =
            interpret_hello(&json!({ "type": "hello", "protocol": 1, "service_version": long }))
        else {
            panic!("should be running");
        };
        assert!(version.chars().count() <= MAX_VERSION && !version.contains('\n'));
        // No version given.
        assert_eq!(
            interpret_hello(&json!({ "type": "hello", "protocol": 1 })),
            SentinelStatus::Running {
                version: "unknown".into()
            }
        );
    }

    #[test]
    fn the_exchange_sends_a_hello_and_reads_one_reply() {
        let reply =
            encode_frame(&json!({ "type": "hello", "protocol": 1, "service_version": "0.3" }))
                .unwrap();
        let mut io = fake(reply);
        assert_eq!(
            hello(&mut io),
            SentinelStatus::Running {
                version: "0.3".into()
            }
        );
        assert_eq!(
            io.sent,
            encode_frame(&json!({ "type": "hello", "protocol": 1 })).unwrap()
        );
    }

    #[test]
    fn bad_answers_are_a_status_not_a_crash() {
        // Nothing comes back.
        assert!(matches!(
            hello(&mut fake(vec![])),
            SentinelStatus::Error { .. }
        ));
        // A header that claims too much.
        let big = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        assert!(matches!(
            hello(&mut fake(big)),
            SentinelStatus::Incompatible { .. }
        ));
        // A body shorter than announced.
        let mut cut = 50u32.to_le_bytes().to_vec();
        cut.extend_from_slice(b"{}");
        assert!(matches!(
            hello(&mut fake(cut)),
            SentinelStatus::Error { .. }
        ));
        // Not JSON.
        let mut junk = 4u32.to_le_bytes().to_vec();
        junk.extend_from_slice(b"nope");
        assert!(matches!(
            hello(&mut fake(junk)),
            SentinelStatus::Incompatible { .. }
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_sentinel_is_absent() {
        assert_eq!(query(), SentinelStatus::Absent);
        assert_eq!(PIPE, r"\\.\pipe\sentinel");
    }
}
