//! Versioned authenticated endpoints. A PID is never termination authority.
//! Backends own a live group anchor and publish an atomic receipt only after stop/reap.
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
            #[serde(rename = "complete")]
            _complete: bool,
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
    pub(super) fn complete(&self, bytes: &[u8]) -> bool {
        self.confirms(bytes)
            && serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .and_then(|v| v.get("complete").and_then(serde_json::Value::as_bool))
                .unwrap_or(false)
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
    request_with_authority(req, nonce, record, None, false)
}
fn request_with_authority(
    req: &Request,
    nonce: &str,
    record: Option<&str>,
    workspace: Option<&super::workspace_authority::WorkspaceAuthority>,
    cleanup: bool,
) -> Result<String, String> {
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
        "workspace": workspace,
        "cleanup": cleanup,
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
    streaming_with_authority(req, nonce, record, None, false)
}
pub(super) fn streaming_with_authority(
    req: &Request,
    nonce: &str,
    record: Option<&str>,
    workspace: Option<&super::workspace_authority::WorkspaceAuthority>,
    cleanup: bool,
) -> Result<String, String> {
    streaming_bounded(req, nonce, record, workspace, cleanup, 64 << 20)
}
pub(super) fn streaming_bounded(
    req: &Request,
    nonce: &str,
    record: Option<&str>,
    workspace: Option<&super::workspace_authority::WorkspaceAuthority>,
    cleanup: bool,
    output_limit: usize,
) -> Result<String, String> {
    if !(1..=(256 << 20)).contains(&output_limit) {
        return Err("invalid standalone capture limit".into());
    }
    let mut value: serde_json::Value = serde_json::from_str(&request_with_authority(
        req, nonce, record, workspace, cleanup,
    )?)
    .map_err(|e| e.to_string())?;
    value["output_limit"] = output_limit.into();
    let data = format!(
        "{}\n",
        serde_json::to_string(&value).map_err(|e| e.to_string())?
    );
    let launch = launcher("stream", "\"$T/request\"");
    Ok(format!(
        "{{\numask 077\nT=$(mktemp -d /tmp/gitcraft-launch.XXXXXXXXXX) || exit 125\ncat >\"$T/request\" <<'GITCRAFT_REQUEST_V1'\n{data}GITCRAFT_REQUEST_V1\n{launch}\n}}\n"
    ))
}

pub(super) fn claim(data: &str) -> String {
    let launch = launcher("claim", "\"$T/request\"");
    format!(
        "{{\numask 077\nT=$(mktemp -d /tmp/gitcraft-launch.XXXXXXXXXX) || exit 125\ncat >\"$T/request\" <<'GITCRAFT_CLAIM_REQUEST_V1'\n{data}GITCRAFT_CLAIM_REQUEST_V1\n{launch}\n}}\n"
    )
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
    fn standalone_capture_limit_is_explicit_bounded_and_does_not_raise_pooled_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let req = Request {
            cwd: None,
            script: "exec git pack-objects --stdout --revs".into(),
            env: vec![],
            unset: vec![],
            stdin: None,
            long: true,
            timeout: Some(std::time::Duration::from_secs(30)),
            test: false,
            apart: false,
        };
        let default = request(&req, "ab", None)?;
        assert!(!default.contains("output_limit"));
        let standalone = streaming_bounded(&req, "ab", None, None, false, 256 << 20)?;
        assert!(standalone.contains("\"output_limit\":268435456"));
        assert!(streaming_bounded(&req, "ab", None, None, false, 0).is_err());
        assert!(streaming_bounded(&req, "ab", None, None, false, (256 << 20) + 1).is_err());
        Ok(())
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
        value["complete"] = true.into();
        value["code"] = 7.into();
        let good = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        assert!(ep.confirms(&good));
        assert!(ep.complete(&good));
        let mut partial = value.clone();
        partial["complete"] = false.into();
        let partial = serde_json::to_vec(&partial).map_err(|e| e.to_string())?;
        assert!(
            ep.confirms(&partial),
            "cleanup proof is independent of output delivery"
        );
        assert!(!ep.complete(&partial));
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
}
