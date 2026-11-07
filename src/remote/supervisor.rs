//! Versioned authenticated endpoints. A PID is never termination authority.
//! Backends own a live group anchor and publish an atomic receipt only after stop/reap.
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::session::Request;
use super::sh_quote;

const PYTHON: &str = include_str!("supervisor/owner.py");
const PERL: &str = include_str!("supervisor/owner.pl");
const ROOT: &str = "/tmp/gitcraft-owned-";
const MAX_INPUT: usize = 64 << 20;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Endpoint {
    v: u8,
    nonce: String,
    path: String,
    token: String,
}
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("v", &self.v)
            .field("nonce", &self.nonce)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
fn hex(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Endpoint {
    fn valid(&self) -> bool {
        self.v == 1
            && hex(&self.nonce, 1, 96)
            && hex(&self.token, 64, 64)
            && self.path.strip_prefix(ROOT).is_some_and(|s| hex(s, 32, 32))
    }
    pub(super) fn parse(line: &[u8], nonce: &str) -> Option<Self> {
        if line.len() > 512 {
            return None;
        }
        let line = std::str::from_utf8(line).ok()?;
        let mut fields = line.split_ascii_whitespace();
        if fields.next()? != "O1" {
            return None;
        }
        let endpoint = Self {
            v: 1,
            nonce: fields.next()?.into(),
            path: fields.next()?.into(),
            token: fields.next()?.into(),
        };
        (fields.next().is_none() && endpoint.nonce == nonce && endpoint.valid()).then_some(endpoint)
    }
    pub(crate) fn from_record(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 2048 {
            return Err("oversized ownership record".into());
        }
        let endpoint: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid ownership record: {e}"))?;
        if !endpoint.valid() {
            return Err("invalid ownership endpoint".into());
        }
        Ok(endpoint)
    }
    pub(super) fn control_script(&self) -> Result<String, String> {
        self.operation_script("control")
    }
    pub(super) fn retire_script(&self) -> Result<String, String> {
        self.operation_script("retire")
    }
    fn operation_script(&self, mode: &str) -> Result<String, String> {
        if !self.valid() {
            return Err("invalid ownership endpoint".into());
        }
        let data = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let launch = launcher(mode, "");
        // Capability goes over stdin, never into a process command line.
        Ok(format!(
            "{{\n{launch}\n}} <<'GITCRAFT_CONTROL_V1'\n{data}\nGITCRAFT_CONTROL_V1\n"
        ))
    }
    pub(super) fn confirms(&self, bytes: &[u8]) -> bool {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Receipt {
            v: u8,
            nonce: String,
            path: String,
            token: String,
            state: String,
            code: i32,
        }
        if bytes.len() > 2048 {
            return false;
        }
        let Ok(receipt) = serde_json::from_slice::<Receipt>(bytes) else {
            return false;
        };
        receipt.v == self.v
            && receipt.nonce == self.nonce
            && receipt.path == self.path
            && receipt.token == self.token
            && receipt.state == "stopped"
            && (0..=255).contains(&receipt.code)
    }
}

/// Shared only within one dispatch. First valid exact-nonce endpoint wins; command output
/// cannot replace an established owner. Debug never includes the capability.
#[derive(Clone, Default)]
pub(crate) struct Lease(Arc<Mutex<Option<Endpoint>>>);
impl Lease {
    pub(crate) fn endpoint(&self) -> Option<Endpoint> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub(super) fn accept(&self, endpoint: Endpoint) -> bool {
        let mut saved = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if saved.is_some() {
            return false;
        }
        *saved = Some(endpoint);
        true
    }
}

/// Remove the out-of-band ownership handshake before output capture or callbacks. Only
/// the first valid exact-nonce endpoint is accepted. Command stderr starts after its gate.
pub(super) struct Marker {
    nonce: String,
    lease: Lease,
    filter: super::private_marker::Filter,
}
impl Marker {
    pub(super) fn new(nonce: String, lease: Lease) -> Self {
        Self {
            nonce,
            lease,
            filter: super::private_marker::Filter::new(b"__GITCRAFT_OWNER__ "),
        }
    }
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.filter.feed(bytes, |hello| {
            if let Some(endpoint) = Endpoint::parse(hello, &self.nonce) {
                self.lease.accept(endpoint)
            } else {
                false
            }
        })
    }
}

/// Probe a backend before dispatching repository code. Core Perl is a fallback on macOS
/// and minimal SSH/WSL hosts without Python; hosts with neither get an actionable preflight
/// failure. Never fall back to sh job control, process discovery, or raw PID termination.
fn launcher(mode: &str, args: &str) -> String {
    let python = sh_quote(PYTHON);
    let perl = sh_quote(PERL);
    format!(
        "if command -v python3 >/dev/null 2>&1 && python3 -c {python} probe </dev/null 2>/dev/null; then\nexec python3 -c {python} {mode} {args}\nelif command -v perl >/dev/null 2>&1 && perl -e {perl} -- probe </dev/null 2>/dev/null; then\nexec perl -e {perl} -- {mode} {args}\nelse\nprintf '%s\\n' 'gitcraft: safe execution needs Python 3 POSIX support or Perl with its core POSIX/JSON modules; install either on this machine' >&2\nexit 125\nfi"
    )
}

pub(super) fn setup() -> String {
    format!("{{\n{}\n}}\n", launcher("serve", ""))
}

pub(super) fn request(req: &Request, nonce: &str, record: Option<&str>) -> Result<String, String> {
    use base64::Engine;
    if !hex(nonce, 1, 96) {
        return Err("invalid request nonce".into());
    }
    if req.stdin.as_ref().is_some_and(|b| b.len() > MAX_INPUT) {
        return Err("remote input exceeds 64 MiB".into());
    }
    let ms = req.time_limit().as_millis();
    if ms == 0 || ms > 7 * 86400 * 1000 {
        return Err("invalid remote execution deadline".into());
    }
    let value = serde_json::json!({
        "nonce": nonce,
        "script": super::session::inline(req, ""),
        "input": base64::engine::general_purpose::STANDARD.encode(req.stdin.as_deref().unwrap_or_default()),
        "milliseconds": ms as u64,
        "record": record,
    });
    Ok(format!(
        "{}\n",
        serde_json::to_string(&value).map_err(|e| e.to_string())?
    ))
}

/// A streaming request file is private and written before exec. Payload/environment/input
/// stay out of argv. fd 0 remains a separate controller lease, never user stdin.
pub(super) fn streaming(
    req: &Request,
    nonce: &str,
    record: Option<&str>,
) -> Result<String, String> {
    let data = request(req, nonce, record)?;
    let launch = launcher("stream", "\"$T/request\"");
    Ok(format!(
        "{{\numask 077\nT=$(mktemp -d /tmp/gitcraft-launch.XXXXXXXXXX) || exit 125\ncat >\"$T/request\" <<'GITCRAFT_REQUEST_V1'\n{data}GITCRAFT_REQUEST_V1\n{launch}\n}}\n"
    ))
}

/// The actual writer treats every write/flush error as potentially delivered. A zero-byte
/// error from a reused buffered transport is still not evidence that a request is safe to
/// replay. No automatic retry after entering this operation.
pub(super) fn write_request(writer: &mut impl Write, script: &[u8]) -> io::Result<()> {
    writer.write_all(script).and_then(|()| writer.flush())
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    fn endpoint() -> Result<Endpoint, String> {
        Endpoint::parse(
            format!("O1 ab {ROOT}{} {}\n", "a".repeat(32), "b".repeat(64)).as_bytes(),
            "ab",
        )
        .ok_or("fixture parse failed".into())
    }
    #[test]
    fn exact_identity_and_full_receipt_are_required() -> Result<(), String> {
        let ep = endpoint()?;
        let hello = format!("O1 ab {} {}", ep.path, ep.token);
        for bad in [
            hello.replace("O1", "O2"),
            hello.replace("ab ", "ac "),
            format!("{hello} extra"),
            hello.replace(&ep.path, "/tmp/../etc/passwd"),
            hello.replace(&ep.token, "123"),
            hello.replace(&ep.token, &"B".repeat(64)),
        ] {
            assert!(Endpoint::parse(bad.as_bytes(), "ab").is_none(), "{bad}");
        }
        let lease = Lease::default();
        assert!(lease.accept(ep.clone()));
        assert!(!lease.accept(ep.clone()));
        assert_eq!(lease.endpoint(), Some(ep.clone()));
        let mut value = serde_json::to_value(&ep).map_err(|e| e.to_string())?;
        value["state"] = "stopped".into();
        value["code"] = 7.into();
        let good = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        assert!(ep.confirms(&good));
        assert!(!ep.confirms(&good[..good.len() - 1]));
        for key in ["token", "nonce", "state"] {
            let mut wrong = value.clone();
            wrong[key] = "wrong".into();
            assert!(!ep.confirms(&serde_json::to_vec(&wrong).map_err(|e| e.to_string())?));
        }
        value["extra"] = true.into();
        assert!(!ep.confirms(&serde_json::to_vec(&value).map_err(|e| e.to_string())?));
        assert!(!format!("{ep:?}").contains(&ep.token));
        Ok(())
    }
    #[test]
    fn split_streaming_handshake_is_private_and_cannot_replace_the_owner() -> Result<(), String> {
        let ep = endpoint()?;
        let handshake = format!("__GITCRAFT_OWNER__ O1 ab {} {}\n", ep.path, ep.token);
        for cut in 0..=handshake.len() {
            let lease = Lease::default();
            let mut marker = Marker::new("ab".into(), lease.clone());
            let mut output = marker.feed(&handshake.as_bytes()[..cut]);
            output.extend(marker.feed(&handshake.as_bytes()[cut..]));
            output.extend(marker.feed(b"Receiving objects: 42%\r"));
            assert_eq!(output, b"Receiving objects: 42%\r");
            assert_eq!(lease.endpoint(), Some(ep.clone()));
        }
        let lease = Lease::default();
        let mut marker = Marker::new("ac".into(), lease.clone());
        assert_eq!(
            marker.feed(handshake.as_bytes()),
            b"gitcraft: invalid ownership handshake\n"
        );
        assert!(lease.endpoint().is_none());
        Ok(())
    }

    #[test]
    fn actual_writer_never_reports_partial_or_flush_failure_as_success() {
        struct Recording {
            left: usize,
            flush_fails: bool,
            sent: Vec<u8>,
        }
        impl Write for Recording {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let n = self.left.min(bytes.len());
                if n == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                self.sent.extend_from_slice(&bytes[..n]);
                self.left -= n;
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                if self.flush_fails {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(())
                }
            }
        }
        let frame = b"{\nrequest\n}\n";
        for (left, flush_fails) in [(0, false), (frame.len() - 1, false), (frame.len(), true)] {
            let mut writer = Recording {
                left,
                flush_fails,
                sent: Vec::new(),
            };
            assert!(write_request(&mut writer, frame).is_err());
            assert_eq!(writer.sent.len(), left.min(frame.len()));
        }
    }
}
