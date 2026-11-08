//! Opt-in native Git/libcurl/netrc/PPID integration, deliberately separate from setup tests.
//! Runtime prerequisites must be supplied as absolute paths; absence is an error, never a pass.
#![deny(
    warnings,
    dead_code,
    clippy::all,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
use super::{Credentials, credential_command_policy};
use crate::processes::{Options, Output, Service};
use crate::tasks::Cancellation;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const SERVER: &str = include_str!("native_https_fixture/server.py");
const USER: &str = "fixture-selected";
const TOKEN: &str = "DUMMY_SELECTED_NATIVE_TOKEN";
const LIMIT: Duration = Duration::from_secs(12);
const CAP: usize = 32 * 1024;

fn options(input: Option<Vec<u8>>, timeout: Duration) -> Options {
    Options {
        input,
        timeout: Some(timeout),
        cancel: Cancellation::default(),
        output_limit: CAP,
        stderr_limit: Some(CAP),
        drain_for: Duration::from_secs(1),
        ..Options::default()
    }
}

// env_clear is fixture isolation, not a change to the production HOME/profile/CA policy.
fn private_command(program: &Path, root: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .current_dir(root)
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("home/xdg"))
        .env("TMPDIR", root.join("tmp"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", root.join("empty-config"))
        .env("GIT_CONFIG_GLOBAL", root.join("profile"))
        .env("GIT_SSL_CAINFO", root.join("ca.pem"))
        .env("GIT_SSL_NO_VERIFY", "false")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/usr/bin/false")
        .env("SSH_ASKPASS", "/usr/bin/false")
        .env("GIT_CEILING_DIRECTORIES", root)
        .env("GIT_CONFIG_COUNT", "0")
        .env("OPENSSL_CONF", root.join("empty-config"))
        .env("LC_ALL", "C");
    command
}

fn git_command(
    git: &Path,
    root: &Path,
    selected_url: &str,
    actual_url: &str,
    args: &[&str],
) -> Result<Command, String> {
    let credentials = Credentials::for_url(selected_url, USER.into(), TOKEN.into())?;
    let mut policy = credential_command_policy(Some(root), &credentials, args)?;
    policy.bind_git(git)?;
    let mut command = private_command(git, root);
    command.args(&policy.arguments).args(args);
    for (key, value) in policy.environment {
        command.env(key, value);
    }
    // Exercise command-only remote resolution and maximally specific hostile auth config.
    command
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "remote.fixture-private.url")
        .env("GIT_CONFIG_VALUE_0", actual_url)
        .env(
            "GIT_CONFIG_KEY_1",
            format!("http.{actual_url}/.proactiveAuth"),
        )
        .env("GIT_CONFIG_VALUE_1", "none")
        .env("GIT_CONFIG_KEY_2", format!("http.{actual_url}/.emptyAuth"))
        .env("GIT_CONFIG_VALUE_2", "true");
    Ok(command)
}

fn complete(service: &Service, command: Command, input: Option<Vec<u8>>) -> Result<Output, String> {
    let out = service.submit(command, options(input, LIMIT))?.wait()?;
    if let Some(why) = &out.stopped {
        return Err(format!("fixture stop={:?}: {why}", out.stop_kind));
    }
    Ok(out)
}

fn success(out: &Output) -> Result<(), String> {
    if !out.status.success() {
        return Err(format!(
            "fixture command failed: {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn required_tool(key: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os(key).map(PathBuf::from).ok_or_else(|| {
        format!("unchecked prerequisite: set {key} to an absolute trusted native executable")
    })?;
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("unchecked prerequisite: invalid {key}"));
    }
    Ok(path)
}

fn write(root: &Path, name: &str, value: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(root.join(name), value).map_err(|e| e.to_string())
}

fn certificates(service: &Service, openssl: &Path, root: &Path) -> Result<(), String> {
    // Provenance: fresh test-only CA and leaf, RSA2048/SHA256, two-day validity,
    // 127.0.0.1 SAN only. No PEM/key/binary is committed and no trust store is edited.
    write(
        root,
        "leaf.ext",
        "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n",
    )?;
    for args in [
        vec![
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-days",
            "2",
            "-subj",
            "/CN=Gitcraft fixture CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
        ],
        vec![
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-subj",
            "/CN=127.0.0.1",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ],
        vec![
            "x509",
            "-req",
            "-sha256",
            "-days",
            "2",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-set_serial",
            "2",
            "-extfile",
            "leaf.ext",
            "-out",
            "server.pem",
        ],
    ] {
        let mut command = private_command(openssl, root);
        command.args(args);
        success(&complete(service, command, None)?)?;
    }
    Ok(())
}

fn cases(service: &Service, git: &Path, root: &Path, port: u16) -> Result<(), String> {
    let url = format!("https://127.0.0.1:{port}/repo.git");
    let ls = ["ls-remote", "fixture-private"];
    // Case 1: actual native Authorization, private alias/count config, netrc and helper conflict.
    let out = complete(service, git_command(git, root, &url, &url, &ls)?, None)?;
    success(&out)?;
    if !String::from_utf8_lossy(&out.stdout)
        .contains("1111111111111111111111111111111111111111\trefs/heads/main")
    {
        return Err("native dumb-Git response did not reach ls-remote".into());
    }
    // Profile remains readable under the production wrapper (no profile/HOME reset).
    let out = complete(
        service,
        git_command(
            git,
            root,
            &url,
            &url,
            &["config", "--get", "fixture.marker"],
        )?,
        None,
    )?;
    success(&out)?;
    if out.stdout != b"private-profile-intact\n" {
        return Err("private profile contract lost".into());
    }
    // Cases 2 and 3: same-origin path redirect and other loopback-origin redirect.
    for path in ["redirect-path.git", "redirect-origin.git"] {
        let redirect = format!("https://127.0.0.1:{port}/{path}");
        let out = complete(
            service,
            git_command(git, root, &redirect, &redirect, &ls)?,
            None,
        )?;
        if out.status.success() {
            return Err(format!("redirect unexpectedly succeeded: {path}"));
        }
    }
    // Case 4: a native transport for the wrong path cannot acquire the selected credential.
    let wrong = format!("https://127.0.0.1:{port}/wrong.git");
    let out = complete(service, git_command(git, root, &url, &wrong, &ls)?, None)?;
    if out.status.success() {
        return Err("wrong-path transport unexpectedly succeeded".into());
    }
    // Case 5: embedded different account refuses before native requests.
    let wrong = format!("https://other-fixture-account@127.0.0.1:{port}/repo.git");
    let out = complete(service, git_command(git, root, &url, &wrong, &ls)?, None)?;
    if out.status.success() || !String::from_utf8_lossy(&out.stderr).contains("credential-free") {
        return Err("wrong-account URL did not explicitly refuse".into());
    }
    // Case 6: actual secondary credential-fill process inherits no designated HTTP owner.
    let input =
        format!("protocol=https\nhost=127.0.0.1:{port}\npath=repo.git\nusername={USER}\n\n");
    let out = complete(
        service,
        git_command(git, root, &url, &url, &["credential", "fill"])?,
        Some(input.into_bytes()),
    )?;
    if out.status.success()
        || !out.stdout.is_empty()
        || !String::from_utf8_lossy(&out.stderr).contains("designated Git HTTP transport")
    {
        return Err("secondary credential fill did not refuse without emitting credentials".into());
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct Request {
    port: u16,
    path: String,
    authorization: String,
}

fn requests(root: &Path, port: u16) -> Result<(), String> {
    use base64::Engine as _;
    let log = std::fs::read(root.join("requests.jsonl")).map_err(|e| e.to_string())?;
    if log.len() > CAP {
        return Err("request log exceeded fixture bound".into());
    }
    let log = std::str::from_utf8(&log).map_err(|e| e.to_string())?;
    let mut repo = 0;
    let mut redirect_path = 0;
    let mut redirect_origin = 0;
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{TOKEN}"))
    );
    for line in log.lines() {
        let request: Request = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if request.port != port || request.authorization != expected {
            return Err(
                "unexpected origin or native Authorization (details retained privately)".into(),
            );
        }
        match request.path.as_str() {
            "/repo.git/info/refs?service=git-upload-pack" | "/repo.git/HEAD" => repo += 1,
            "/redirect-path.git/info/refs?service=git-upload-pack" => redirect_path += 1,
            "/redirect-origin.git/info/refs?service=git-upload-pack" => redirect_origin += 1,
            _ => return Err("refused/sink path received a native request".into()),
        }
    }
    if repo == 0 || redirect_path != 1 || redirect_origin != 1 {
        return Err("native request coverage incomplete".into());
    }
    if root.join("home/ambient-helper-ran").exists() {
        return Err("ambient helper was consulted".into());
    }
    Ok(())
}

fn run_fixture(root: &Path, git: &Path, python: &Path, openssl: &Path) -> Result<(), String> {
    let service = crate::processes::owned_fixture_service().map_err(|e| e.to_string())?;
    certificates(&service, openssl, root)?;
    let mut command = private_command(python, root);
    command
        .args([OsStr::new("-I"), OsStr::new("-S"), OsStr::new("-u")])
        .arg(root.join("server.py"))
        .arg(root);
    let (send, receive) = mpsc::sync_channel(1);
    let mut startup = Vec::new();
    let mut sent = false;
    let mut server_options = options(None, Duration::from_secs(105));
    server_options.stdout = Some(Box::new(move |chunk| {
        if sent {
            return;
        }
        if startup.len() + chunk.len() > 256 {
            let _ = send.try_send(Err("server startup exceeded 256 bytes".to_string()));
            sent = true;
            return;
        }
        startup.extend_from_slice(chunk);
        if let Some(end) = startup.iter().position(|b| *b == b'\n') {
            let result =
                serde_json::from_slice::<[u16; 2]>(&startup[..end]).map_err(|e| e.to_string());
            let _ = send.try_send(result);
            sent = true;
        }
    }));
    let ticket = service.submit(command, server_options)?;
    // Always stop and collect this exact owned ticket, including startup/case errors.
    let result: Result<u16, String> = (|| {
        let ports = receive
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| format!("TLS startup unconfirmed: {e}"))??;
        if ports.contains(&0) || ports[0] == ports[1] {
            return Err("invalid loopback port receipt".into());
        }
        cases(&service, git, root, ports[0])?;
        Ok(ports[0])
    })();
    let stop = write(root, "stop", b"stop\n");
    if stop.is_err() {
        ticket.cancel();
    }
    let shutdown = ticket.wait().and_then(|out| {
        if let Some(why) = &out.stopped {
            return Err(format!("TLS shutdown stop={:?}: {why}", out.stop_kind));
        }
        success(&out)?;
        if !out.stdout.ends_with(b"CLOSED\n") {
            return Err("TLS shutdown receipt missing".into());
        }
        Ok(())
    });
    // Retain both primary failure and unknown cleanup information rather than short-circuiting.
    match (result, stop, shutdown) {
        (Ok(port), Ok(()), Ok(())) => requests(root, port),
        (result, stop, shutdown) => Err(format!(
            "cases={result:?}; stop={stop:?}; shutdown={shutdown:?}"
        )),
    }
}

#[test]
#[ignore = "one owned native HTTPS gate; explicit parent containment approval and absolute Git>=2.46/Python3/OpenSSL prerequisites required"]
fn native_https_selected_credential_contract() -> Result<(), String> {
    let before = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let git = required_tool("GITCRAFT_FIXTURE_GIT")?;
    let python = required_tool("GITCRAFT_FIXTURE_PYTHON")?;
    let openssl = required_tool("GITCRAFT_FIXTURE_OPENSSL")?;
    // No automatic deletion on failure/uncertain cleanup. Even success retains empty
    // production wrapper directories and owned CA/log evidence for parent inspection.
    let root = tempfile::Builder::new()
        .prefix("gitcraft-native-https-")
        .tempdir()
        .map_err(|e| e.to_string())?
        .keep();
    eprintln!("owned fixture evidence retained at {}", root.display());
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for name in ["home", "home/xdg", "tmp"] {
        std::fs::create_dir(root.join(name)).map_err(|e| e.to_string())?;
    }
    write(&root, "empty-config", b"")?;
    write(
        &root,
        "profile",
        r#"[fixture]
    marker = private-profile-intact
[credential]
    helper = "!f() { printf ran > \"$HOME/ambient-helper-ran\"; printf 'username=wrong-helper\\npassword=DUMMY_WRONG_HELPER\\n'; }; f"
"#,
    )?;
    write(
        &root,
        "home/.netrc",
        "machine 127.0.0.1 login wrong-netrc password DUMMY_WRONG_NETRC\n",
    )?;
    std::fs::set_permissions(
        root.join("home/.netrc"),
        std::fs::Permissions::from_mode(0o600),
    )
    .map_err(|e| e.to_string())?;
    write(&root, "server.py", SERVER)?;
    let profile = std::fs::read(root.join("profile")).map_err(|e| e.to_string())?;
    let result = run_fixture(&root, &git, &python, &openssl);
    let after_profile = std::fs::read(root.join("profile")).map_err(|e| {
        format!("private profile verification failed: {e}; fixture result={result:?}")
    })?;
    let unchanged = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>() == before
        && after_profile == profile;
    if !unchanged {
        return Err(format!(
            "parent environment/private profile changed; result={result:?}"
        ));
    }
    result
}

#[test]
fn native_https_plan_records_private_contract_without_starting_tools() -> Result<(), String> {
    let root = Path::new("/recording/owned");
    let git = Path::new("/recording/native/git");
    let url = "https://127.0.0.1:44443/repo.git";
    let command = git_command(git, root, url, url, &["ls-remote", "fixture-private"])?;
    assert_eq!(command.get_program(), git);
    assert_eq!(command.get_current_dir(), Some(root));
    let env = command
        .get_envs()
        .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
        .collect::<std::collections::BTreeMap<OsString, Option<OsString>>>();
    for (key, value) in [
        ("HOME", "/recording/owned/home"),
        ("GIT_CONFIG_GLOBAL", "/recording/owned/profile"),
        ("GIT_SSL_CAINFO", "/recording/owned/ca.pem"),
        ("GITCRAFT_GIT_ORIGINAL", "/recording/native/git"),
        ("GIT_CONFIG_COUNT", "3"),
        ("GIT_CONFIG_VALUE_0", url),
    ] {
        assert_eq!(env.get(OsStr::new(key)), Some(&Some(OsString::from(value))));
    }
    assert!(!env.contains_key(OsStr::new("GITCRAFT_CREDENTIAL_OWNER")));
    assert!(
        !command
            .get_args()
            .any(|a| a.to_string_lossy().contains(TOKEN))
    );
    assert!(!SERVER.contains(TOKEN));
    let plan = options(None, LIMIT);
    assert_eq!(plan.timeout, Some(LIMIT));
    assert_eq!(plan.output_limit, CAP);
    assert_eq!(plan.stderr_limit, Some(CAP));
    assert!(!plan.cancel.is_cancelled());
    Ok(())
}
