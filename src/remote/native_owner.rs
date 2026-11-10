//! Packaged native endpoint for local durable work, including Git-for-Windows. The
//! controller process survives app death; only its shared Service owns target groups/Jobs.
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MARKER: &[u8] = b"__GITCRAFT_NATIVE_OWNER__ ";
const READY: &str = "__GITCRAFT_NATIVE_READY__";
const PREFIX: &str = "gitcraft-native-owned-";
const MAX_REQUEST: usize = 96 << 20;

#[derive(Serialize, Deserialize)]
enum Word {
    Unix(Vec<u8>),
    Windows(Vec<u16>),
}
impl Word {
    fn capture(word: &OsStr) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::Unix(word.as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self::Windows(word.encode_wide().collect())
        }
    }
    fn restore(&self) -> Result<OsString, String> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            match self {
                Self::Unix(bytes) => Ok(OsString::from_vec(bytes.clone())),
                _ => Err("foreign native command encoding".into()),
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            match self {
                Self::Windows(words) => Ok(OsString::from_wide(words)),
                _ => Err("foreign native command encoding".into()),
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    nonce: String,
    program: Word,
    args: Vec<Word>,
    env: Vec<(Word, Option<Word>)>,
    cwd: Option<Word>,
    record: PathBuf,
    milliseconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace: Option<super::workspace_authority::WorkspaceAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cleanup: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Endpoint {
    v: u8,
    nonce: String,
    path: PathBuf,
    port: u16,
    token: String,
}
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEndpoint")
            .field("v", &self.v)
            .field("nonce", &self.nonce)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    endpoint: Endpoint,
    state: String,
    code: i64,
    complete: bool,
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn native_exit_code(code: i64, windows: bool) -> bool {
    if windows {
        i32::try_from(code).is_ok()
    } else {
        (0..=255).contains(&code)
    }
}
fn random_hex(length: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; length];
    gitcraft_os::random_bytes(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Endpoint {
    fn valid(&self) -> bool {
        self.v == 2
            && self.port > 0
            && hex(&self.nonce, 64)
            && hex(&self.token, 64)
            && self.path.parent() == Some(std::env::temp_dir().as_path())
            && self
                .path
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|s| s.strip_prefix(PREFIX))
                .is_some_and(|s| hex(s, 32))
    }
    pub(crate) fn from_record(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 2048 {
            return Err("oversized native owner record".into());
        }
        let result: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !result.valid() {
            return Err("invalid native owner endpoint".into());
        }
        Ok(result)
    }
    pub(super) fn confirms(&self, bytes: &[u8]) -> bool {
        if bytes.len() > 4096 {
            return false;
        }
        serde_json::from_slice::<Receipt>(bytes).is_ok_and(|r| {
            r.endpoint == *self && r.state == "stopped" && native_exit_code(r.code, cfg!(windows))
        })
    }
    pub(super) fn terminal(&self) -> Result<Vec<u8>, String> {
        let bytes = crate::safe_read::read_inside(&self.path, "terminal", 4097)
            .ok_or("native terminal receipt unavailable")?;
        if !self.confirms(&bytes) {
            return Err("unconfirmed native terminal receipt".into());
        }
        Ok(bytes)
    }
    pub(super) fn retire_proven(&self, receipt: &[u8]) -> Result<(), String> {
        if !self.valid() || !self.confirms(receipt) {
            return Err("invalid cached native retirement proof".into());
        }
        let directory = match gitcraft_os::PrivateDirectory::open(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        match directory.remove_checked("terminal", receipt) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub(crate) fn stop(&self) -> Result<(), String> {
        if !self.valid() {
            return Err("invalid native owner endpoint".into());
        }
        if self.terminal().is_ok() {
            return Ok(());
        }
        match self.stop_exchange() {
            Ok(()) => Ok(()),
            Err(error) => self
                .terminal()
                .map(|_| ())
                .map_err(|proof| format!("{error}; atomic cleanup proof unavailable: {proof}")),
        }
    }
    fn stop_exchange(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut socket = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))
            .map_err(|_| "native owner unavailable; cleanup unknown")?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut request = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        request.push(b'\n');
        super::deadline_io::write_frame(
            &mut socket,
            &request,
            || Instant::now() >= deadline,
            || std::thread::sleep(Duration::from_millis(2)),
        )
        .map_err(|e| e.to_string())?;
        let answer = super::deadline_io::read_frame(
            &mut socket,
            4096,
            || Instant::now() >= deadline,
            || std::thread::sleep(Duration::from_millis(2)),
        )
        .map_err(|e| e.to_string())?;
        if !self.confirms(&answer) {
            return Err("native stop was not acknowledged".into());
        }
        self.terminal().map(|_| ())
    }
}

/// Start only the packaged helper; Command remains private until all safeguards are set.
/// Capturing get_envs is specific to git's local_command, which never uses env_clear.
#[cfg(test)]
pub(crate) fn command(
    command: Command,
    record: &Path,
    limit: Duration,
) -> Result<NativeCommand, String> {
    command_with_authority(command, record, limit, None)
}
pub(crate) fn command_with_authority(
    command: Command,
    record: &Path,
    limit: Duration,
    workspace: Option<super::workspace_authority::WorkspaceAuthority>,
) -> Result<NativeCommand, String> {
    let nonce = random_hex(32)?;
    let request = Request {
        nonce: nonce.clone(),
        program: Word::capture(command.get_program()),
        args: command.get_args().map(Word::capture).collect(),
        env: command
            .get_envs()
            .map(|(k, v)| (Word::capture(k), v.map(Word::capture)))
            .collect(),
        cwd: command
            .get_current_dir()
            .map(|p| Word::capture(p.as_os_str())),
        record: record.into(),
        milliseconds: u64::try_from(limit.as_millis()).map_err(|e| e.to_string())?,
        workspace,
        input: None,
        cleanup: false,
        directory: None,
    };
    let mut input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    input.push(b'\n');
    let mut transport = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(not(test))]
    transport.arg("--gitcraft-owned-endpoint");
    #[cfg(test)]
    {
        transport
            .args([
                "--exact",
                "remote::native_owner::tests::helper_entrypoint",
                "--nocapture",
            ])
            .env("GITCRAFT_NATIVE_HELPER", "1");
    }
    Ok(NativeCommand {
        command: transport,
        input,
        nonce,
        endpoint: Arc::default(),
    })
}
pub(crate) struct NativeCommand {
    command: Command,
    input: Vec<u8>,
    nonce: String,
    endpoint: Arc<Mutex<Option<Endpoint>>>,
}
impl NativeCommand {
    pub(crate) fn with_input(
        mut self,
        input: Option<&[u8]>,
        cleanup: bool,
    ) -> Result<Self, String> {
        use base64::Engine;
        if input.is_some_and(|b| b.len() > 64 << 20) {
            return Err("native durable input exceeds 64 MiB".into());
        }
        let mut request: Request =
            serde_json::from_slice(&self.input).map_err(|e| e.to_string())?;
        request.input = input.map(|b| base64::engine::general_purpose::STANDARD.encode(b));
        request.cleanup = cleanup;
        self.input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        self.input.push(b'\n');
        if self.input.len() > MAX_REQUEST {
            return Err("native request exceeds 96 MiB".into());
        }
        Ok(self)
    }
    pub(crate) fn with_directory(mut self, directory: Option<&Path>) -> Result<Self, String> {
        let mut request: Request =
            serde_json::from_slice(&self.input).map_err(|e| e.to_string())?;
        request.directory = directory.map(Path::to_path_buf);
        self.input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        self.input.push(b'\n');
        Ok(self)
    }
    pub(crate) fn configure(self, options: &mut crate::processes::Options) -> Command {
        options.native_controller = true;
        options.hold_input = true;
        options.input = Some(self.input);
        options.stdout_ready = Some(READY);
        let endpoint = self.endpoint.clone();
        let nonce = self.nonce;
        let mut filter = super::private_marker::Filter::new(MARKER);
        options.stderr_filter = Some(Box::new(move |bytes| {
            filter.feed(bytes, |json| {
                if let Ok(ep) = Endpoint::from_record(json)
                    && ep.nonce == nonce
                {
                    *endpoint
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ep);
                    true
                } else {
                    false
                }
            })
        }));
        let endpoint = self.endpoint.clone();
        options.complete_remote = Some(Box::new(move || {
            let endpoint = endpoint
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .ok_or("missing native completion endpoint")?;
            let receipt: Receipt =
                serde_json::from_slice(&endpoint.terminal()?).map_err(|e| e.to_string())?;
            if receipt.complete {
                Ok(())
            } else {
                Err("native command stopped but output delivery is incomplete".into())
            }
        }));
        let endpoint = self.endpoint.clone();
        options.stop_ready = Some(Box::new(move || {
            endpoint
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
        }));
        let endpoint = self.endpoint;
        options.stop_remote = Some(Box::new(move || {
            endpoint
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .ok_or("native owner handshake missing; cleanup unknown")?
                .stop()
        }));
        self.command
    }
}
/// Called before normal app initialization; no window or persisted app state is touched.
pub(crate) fn entrypoint() -> i32 {
    // Error paths retain actual mutation authority through joined Service cleanup.
    // The external record remains if cleanup cannot be confirmed; a new claimant must
    // recover that exact owner before destructive work, even after this helper exits.
    let mut fence = None;
    let code = match helper(&mut fence) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gitcraft owned endpoint: {error}");
            125
        }
    };
    crate::processes::shutdown();
    crate::processes::shutdown_cleanup();
    drop(fence);
    gitcraft_os::retire_process_helpers();
    code
}

#[cfg(test)]
mod tests;

mod controller;
use controller::*;
