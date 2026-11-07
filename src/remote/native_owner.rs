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
    fn confirms(&self, bytes: &[u8]) -> bool {
        if bytes.len() > 4096 {
            return false;
        }
        serde_json::from_slice::<Receipt>(bytes).is_ok_and(|r| {
            r.endpoint == *self && r.state == "stopped" && native_exit_code(r.code, cfg!(windows))
        })
    }
    fn terminal(&self) -> Result<Vec<u8>, String> {
        let bytes = crate::safe_read::read_inside(&self.path, "terminal", 4097)
            .ok_or("native terminal receipt unavailable")?;
        if !self.confirms(&bytes) {
            return Err("unconfirmed native terminal receipt".into());
        }
        Ok(bytes)
    }
    pub(crate) fn stop(&self) -> Result<(), String> {
        if !self.valid() {
            return Err("invalid native owner endpoint".into());
        }
        if self.terminal().is_ok() {
            return Ok(());
        }
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut socket = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))
            .map_err(|_| "native owner unavailable; cleanup unknown")?;
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        let mut request = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        request.push(b'\n');
        socket.write_all(&request).map_err(|e| e.to_string())?;
        let mut answer = Vec::new();
        BufReader::new(socket)
            .take(4097)
            .read_until(b'\n', &mut answer)
            .map_err(|e| e.to_string())?;
        if !self.confirms(&answer) {
            return Err("native stop was not acknowledged".into());
        }
        self.terminal().map(|_| ())
    }
    pub(crate) fn retire(&self) -> Result<(), String> {
        if !self.valid() {
            return Err("invalid native endpoint".into());
        }
        self.terminal()?;
        std::fs::remove_file(self.path.join("terminal")).map_err(|e| e.to_string())?;
        std::fs::remove_dir(&self.path).map_err(|e| e.to_string())
    }
}

/// Start only the packaged helper; Command remains private until all safeguards are set.
/// Capturing get_envs is specific to git's local_command, which never uses env_clear.
pub(crate) fn command(
    command: Command,
    record: &Path,
    limit: Duration,
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
fn private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}
fn helper() -> Result<i32, String> {
    gitcraft_os::detach_controller().map_err(|e| e.to_string())?;
    let mut controller = BufReader::new(std::io::stdin());
    let mut line = Vec::new();
    controller
        .by_ref()
        .take(1 << 20)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if line.len() >= 1 << 20 || !line.ends_with(b"\n") {
        return Err("invalid native request".into());
    }
    let request: Request = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
    if !hex(&request.nonce, 64)
        || request.milliseconds == 0
        || request.milliseconds > 7 * 86400 * 1000
    {
        return Err("invalid native request identity/deadline".into());
    }
    let path = std::env::temp_dir().join(format!("{PREFIX}{}", random_hex(16)?));
    gitcraft_os::create_private_directory(&path).map_err(|e| e.to_string())?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let endpoint = Endpoint {
        v: 2,
        nonce: request.nonce,
        path: path.clone(),
        port: listener.local_addr().map_err(|e| e.to_string())?.port(),
        token: random_hex(32)?,
    };
    let bytes = serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?;
    // Exclusive record creation precedes target submission. Workspace created its private
    // directory, with owner-only inherited ACLs on Windows and mode 0700 on Unix.
    private_file(&request.record, &bytes)?;
    println!("\n{READY}");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    std::io::stderr()
        .write_all(MARKER)
        .and_then(|()| std::io::stderr().write_all(&bytes))
        .and_then(|()| std::io::stderr().write_all(b"\n"))
        .and_then(|()| std::io::stderr().flush())
        .map_err(|e| e.to_string())?;
    let cancel = crate::tasks::Cancellation::default();
    let eof = cancel.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        loop {
            match controller.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    eof.cancel();
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    let mut command = Command::new(request.program.restore()?);
    for word in request.args {
        command.arg(word.restore()?);
    }
    if let Some(cwd) = request.cwd {
        command.current_dir(cwd.restore()?);
    }
    for (key, value) in request.env {
        let key = key.restore()?;
        if let Some(value) = value {
            command.env(key, value.restore()?);
        } else {
            command.env_remove(key);
        }
    }
    command.env_remove("GITCRAFT_NATIVE_HELPER");
    let ticket = crate::processes::service()?.submit(
        command,
        crate::processes::Options {
            cancel: cancel.clone(),
            timeout: Some(Duration::from_millis(request.milliseconds)),
            ..Default::default()
        },
    )?;
    let mut clients = Vec::new();
    let start = Instant::now();
    let result = loop {
        if let Some(result) = ticket.try_result() {
            break result?;
        }
        if start.elapsed() > Duration::from_millis(request.milliseconds) + Duration::from_secs(10) {
            cancel.cancel();
        }
        match listener.accept() {
            Ok((mut socket, _)) => {
                socket
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .map_err(|e| e.to_string())?;
                socket
                    .set_write_timeout(Some(Duration::from_millis(100)))
                    .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                let read = BufReader::new(&mut socket)
                    .take(2049)
                    .read_until(b'\n', &mut bytes);
                if read.is_ok()
                    && Endpoint::from_record(&bytes).is_ok_and(|other| other == endpoint)
                {
                    cancel.cancel();
                    if clients.len() < 4 {
                        clients.push(socket);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // A Service error is not acknowledged. A completed cancellation confirms the actual
    // anchored group/Job was stopped/reaped before a durable receipt is published.
    if result.stop_kind == Some(crate::processes::StopKind::Uncertain) {
        return Err("native cleanup unconfirmed".into());
    }
    let code = result.status.code().unwrap_or(125);
    let receipt = Receipt {
        endpoint,
        state: "stopped".into(),
        code: i64::from(code),
        complete: result.stopped.is_none(),
    };
    let mut bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    private_file(&path.join("terminal.new"), &bytes)?;
    std::fs::rename(path.join("terminal.new"), path.join("terminal")).map_err(|e| e.to_string())?;
    for mut client in clients {
        let _ = client.write_all(&bytes);
    }
    std::io::stdout()
        .write_all(&result.stdout)
        .and_then(|()| std::io::stderr().write_all(&result.stderr))
        .map_err(|e| e.to_string())?;
    Ok(code)
}
/// Called before normal app initialization; no window or persisted app state is touched.
pub(crate) fn entrypoint() -> i32 {
    let code = match helper() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gitcraft owned endpoint: {error}");
            125
        }
    };
    crate::processes::shutdown();
    crate::processes::shutdown_cleanup();
    gitcraft_os::retire_process_helpers();
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_native_exit_codes_preserve_windows_status_range() {
        for code in [0, 7, 255] {
            assert!(native_exit_code(code, false) && native_exit_code(code, true));
        }
        for code in [
            256,
            65536,
            -1073741819,
            i64::from(i32::MIN),
            i64::from(i32::MAX),
        ] {
            assert!(native_exit_code(code, true));
            assert!(!native_exit_code(code, false));
        }
        assert!(!native_exit_code(i64::MAX, true));
    }
    #[test]
    fn helper_entrypoint() {
        if std::env::var_os("GITCRAFT_NATIVE_HELPER").is_some() {
            std::process::exit(entrypoint());
        }
    }
    #[test]
    fn recording_endpoint_refuses_wrong_identity_and_extra_fields() -> Result<(), String> {
        let ep = Endpoint {
            v: 2,
            nonce: "a".repeat(64),
            path: std::env::temp_dir().join(format!("{PREFIX}{}", "b".repeat(32))),
            port: 4000,
            token: "c".repeat(64),
        };
        let bytes = serde_json::to_vec(&ep).map_err(|e| e.to_string())?;
        assert_eq!(Endpoint::from_record(&bytes)?, ep);
        let receipt = serde_json::to_vec(&Receipt {
            endpoint: ep.clone(),
            state: "stopped".into(),
            code: 7,
            complete: true,
        })
        .map_err(|e| e.to_string())?;
        assert!(ep.confirms(&receipt));
        assert!(!ep.confirms(&receipt[..receipt.len() - 1]));
        let mut value = serde_json::to_value(&ep).map_err(|e| e.to_string())?;
        value["extra"] = true.into();
        assert!(
            Endpoint::from_record(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err()
        );
        value.as_object_mut().ok_or("not object")?.remove("extra");
        value["path"] = "/etc/passwd".into();
        assert!(
            Endpoint::from_record(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err()
        );
        assert!(!format!("{ep:?}").contains(&ep.token));
        Ok(())
    }
    #[test]
    fn recording_only_native_helper_gets_controller_mode() -> Result<(), String> {
        let mut cmd = Command::new("git");
        cmd.args(["-c", "alias.test=!echo x", "test"]);
        let native = command(
            cmd,
            Path::new("/recording-only/0.owner"),
            Duration::from_secs(1),
        )?;
        let mut options = crate::processes::Options::default();
        assert!(!options.native_controller);
        let transport = native.configure(&mut options);
        assert!(options.native_controller && options.hold_input && options.stop_remote.is_some());
        assert_eq!(
            transport.get_program(),
            std::env::current_exe().map_err(|e| e.to_string())?
        );
        Ok(())
    }
    #[test]
    fn recording_native_words_roundtrip_without_loss() -> Result<(), String> {
        for word in [
            OsStr::new("space ü"),
            OsStr::new("$(not-a-command)"),
            OsStr::new(""),
        ] {
            assert_eq!(Word::capture(word).restore()?, word);
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let word = OsStr::from_bytes(b"invalid-\xff");
            assert_eq!(Word::capture(word).restore()?, word);
        }
        Ok(())
    }
}
