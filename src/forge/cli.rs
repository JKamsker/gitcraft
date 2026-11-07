//! Running a forge's own command-line tool (`gh`, `glab`), which holds the user's login.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{Forge, Hosting};

/// Runs `tool` in `repo`; `noun` says what it's for when it isn't installed.
pub(super) fn run(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
) -> Result<String, String> {
    run_with_input(tool, noun, repo, hosting, args, None)
}

/// [`run`], with `input` on its stdin.
pub(super) fn run_with_input(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, String> {
    let (mut cmd, not_installed) = prepared(tool, noun, repo, hosting, args)?;
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, input) {
        return answer;
    }
    let out = match input {
        None => cmd.output().map_err(not_installed)?,
        Some(input) => {
            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(not_installed)?;
            if let Some(mut stdin) = child.stdin.take() {
                // A tool that stops reading has stopped for a reason, which its exit code and
                // error output below tell better than the broken pipe would.
                let _ = stdin.write_all(input.as_bytes());
            }
            child.wait_with_output().map_err(|e| e.to_string())?
        }
    };
    answer(tool, &out)
}

/// [`run`], stopped when it hasn't finished `within` that long: for what nobody waits on (the
/// inbox), where a tool that hangs would otherwise stay until the app ends.
pub(super) fn run_within(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    within: Duration,
) -> Result<String, String> {
    let (cmd, not_installed) = prepared(tool, noun, repo, hosting, args)?;
    within_command(tool, cmd, not_installed, within)
}

fn within_command(
    tool: &str,
    mut cmd: Command,
    not_installed: impl Fn(std::io::Error) -> String,
    within: Duration,
) -> Result<String, String> {
    use std::io::Read;
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, None) {
        return answer;
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(not_installed)?;
    // Read as it comes, or a long answer would fill the pipe and the tool wait on it forever.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let until = Instant::now() + within;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // The readers are left: what the tool started may hold its pipes a while yet.
                return Err(format!(
                    "{tool} didn't finish within {} seconds",
                    within.as_secs()
                ));
            }
            Err(e) => return Err(format!("couldn't wait for {tool}: {e}")),
        }
    };
    let out = std::process::Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    answer(tool, &out)
}

/// What the tool printed, or why it failed.
fn answer(tool: &str, out: &std::process::Output) -> Result<String, String> {
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(failure(tool, out))
    }
}

/// The command that runs `tool` with `args` about `repo` (here, or named for one on another
/// machine), and what to say when it can't be started.
fn prepared(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
) -> Result<(Command, impl Fn(std::io::Error) -> String), String> {
    let mut cmd = tool_command(tool);
    let forge = forge_of(tool)?;
    if forge != hosting.forge || !plain_path(&hosting.path) || !plain_host(&hosting.host) {
        return Err(format!(
            "{tool} needs a valid selected forge host and owner/repo path"
        ));
    }
    let authority = match forge {
        Forge::GitLab => super::gitlab::api_authority(hosting),
        _ => hosting.host.clone(),
    };
    if !plain_host(&authority) || !same_host(&authority, &hosting.host) {
        return Err("The selected forge has an invalid web host".to_owned());
    }
    let remote = crate::remote::parse(repo).is_some();
    if remote {
        let remotes = remote_catalog(repo)?;
        validate_remote_target(&remotes, forge, &authority, &hosting.path)?;
        // A named target bypasses the CLI's usual known-host filtering. Only an explicitly
        // configured login (never an inherited global token) may authorize an SSH-tab host.
        if !trusted_host(forge, tool, &authority)? {
            return Err(format!(
                "{tool} isn't signed in to {authority}: `{tool} auth login --hostname {authority}` to use it"
            ));
        }
    }
    if named_hosts(args)
        .iter()
        .any(|host| !host.eq_ignore_ascii_case(&authority))
    {
        return Err("The command names a different host than the selected forge".to_owned());
    }
    let mut args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let api = args.first().is_some_and(|s| s == "api");
    // These switches are command-family specific. Neither tool's `api` accepts --repo.
    // Insert before `--`, which release uploads use to protect asset filenames.
    let at = args.iter().position(|s| s == "--").unwrap_or(args.len());
    if api {
        if !args.iter().any(|s| s == "--hostname") {
            args.splice(at..at, ["--hostname".to_owned(), authority.clone()]);
        }
    } else {
        let target = match forge {
            Forge::GitLab => format!("{}/{}", hosting.web.trim_end_matches('/'), hosting.path),
            _ => format!("{}/{}", hosting.host, hosting.path),
        };
        if args.iter().any(|s| matches!(s.as_str(), "--repo" | "-R")) {
            return Err(
                "Forge commands must use the selected repository, without a repository override"
                    .to_owned(),
            );
        }
        args.splice(at..at, ["--repo".to_owned(), target]);
    }
    cmd.current_dir(if remote {
        std::env::temp_dir()
    } else {
        repo.to_owned()
    })
    .args(args);
    match forge {
        Forge::GitLab => {
            cmd.env("GITLAB_HOST", &authority)
                .env("GITLAB_API_HOST", &authority);
        }
        _ => {
            cmd.env("GH_REPO", format!("{}/{}", hosting.host, hosting.path));
        }
    }
    let here = remote || repo.is_dir();
    let (tool, noun, shown) = (tool.to_owned(), noun.to_owned(), repo.display().to_string());
    let not_installed = move |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound if here => {
            format!("{tool} isn't installed: gitcraft uses it for {noun}s")
        }
        _ => format!("couldn't run {tool} in {shown}: {e}"),
    };
    Ok((cmd, not_installed))
}

/// The forge whose tool `tool` is.
fn forge_of(tool: &str) -> Result<Forge, String> {
    match tool {
        "gh" => Ok(Forge::GitHub),
        "glab" => Ok(Forge::GitLab),
        _ => Err(format!(
            "{tool} can't be run for a repository on another machine"
        )),
    }
}

/// Re-read the SSH repository's catalog before each command; selection is a host AND path,
/// rather than an implicit origin. A changed/removed URL requires refreshing the tab.
fn remote_catalog(repo: &Path) -> Result<Vec<crate::git::Remote>, String> {
    #[cfg(test)]
    if let Some(remotes) = recording::remotes() {
        return Ok(remotes);
    }
    crate::git::remotes(repo)
}

fn validate_remote_target(
    remotes: &[crate::git::Remote],
    forge: Forge,
    host: &str,
    path: &str,
) -> Result<(), String> {
    if remotes.iter().any(|remote| {
        tool_location(forge, &remote.url)
            .is_some_and(|(h, p)| h.eq_ignore_ascii_case(host) && p == path)
    }) {
        Ok(())
    } else {
        Err("The selected forge remote changed or was removed: refresh the tab".to_owned())
    }
}

fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.', ':'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// Whether `path` is a plain `owner/repo` (`group/sub/repo`): never a URL, an `scp`-style
/// address or an option, which `glab --repo` would take as another host.
fn plain_path(path: &str) -> bool {
    path.contains('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with(['-', '.'])
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// The hosts `args` name: the value of `--hostname`, and the host of a `-R` / `--repo` of the
/// form `host/owner/repo`.
fn named_hosts(args: &[&str]) -> Vec<String> {
    let mut hosts = Vec::new();
    for pair in args.windows(2) {
        match pair[0] {
            "--hostname" => hosts.push(pair[1].to_owned()),
            "-R" | "--repo" if pair[1].matches('/').count() >= 2 => {
                hosts.push(pair[1].split('/').next().unwrap_or_default().to_owned());
            }
            _ => {}
        }
    }
    hosts
}

/// Whether two hosts are the same, ports aside (`glab api --hostname` takes none).
fn same_host(a: &str, b: &str) -> bool {
    let name = |h: &str| h.split(':').next().unwrap_or(h).to_lowercase();
    name(a) == name(b)
}

/// The host and `owner/repo` the tool is given for remote `url`: GitLab's host with the port of
/// an `http(s)` URL, an SSH port left out (it says nothing of the web pages).
pub(crate) fn tool_location(forge: Forge, url: &str) -> Option<(String, String)> {
    let (host, path) = super::location(url)?;
    let host = match (forge, http_port(url)) {
        (Forge::GitLab, Some(port)) => format!("{host}:{port}"),
        _ => host,
    };
    Some((host, path))
}

/// The port of an `http(s)` URL that names one.
fn http_port(url: &str) -> Option<&str> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (_, port) = host.rsplit_once(':')?;
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).then_some(port)
}

/// How long the tool may take to say whether it is signed in to a host.
const AUTH_CHECK_FOR: Duration = Duration::from_secs(10);
/// How long a host the tool isn't signed in to is taken as such before it is asked again (the
/// user may sign in meanwhile).
const NOT_SIGNED_IN_FOR: Duration = Duration::from_secs(60);

/// Whether `forge`'s tool may be pointed at `host`: the forge's own host, or one the tool is
/// signed in to. A host that isn't a plain name (`a.b:8443` at most) never is. What the tool
/// said is remembered: yes while the app runs, no for [`NOT_SIGNED_IN_FOR`].
/// An error when the tool can't be run at all (not installed).
fn trusted_host(forge: Forge, tool: &str, host: &str) -> Result<bool, String> {
    use std::sync::{Mutex, OnceLock};
    type Known = Mutex<HashMap<(Forge, String), (bool, Instant)>>;
    static KNOWN: OnceLock<Known> = OnceLock::new();
    if !plain_host(host) {
        return Ok(false);
    }
    let own = match forge {
        Forge::GitLab => "gitlab.com",
        _ => "github.com",
    };
    if host.eq_ignore_ascii_case(own) {
        return Ok(true);
    }
    let known = KNOWN.get_or_init(Known::default);
    let lock = || {
        known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let key = (forge, host.to_lowercase());
    if let Some(&(signed_in, at)) = lock().get(&key)
        && (signed_in || at.elapsed() < NOT_SIGNED_IN_FOR)
    {
        return Ok(signed_in);
    }
    let signed_in = signed_in_to(tool, host)?;
    lock().insert(key, (signed_in, Instant::now()));
    Ok(signed_in)
}

/// Whether `tool` says it is signed in to `host`; no when it doesn't say so within
/// [`AUTH_CHECK_FOR`] (it is stopped).
fn signed_in_to(tool: &str, host: &str) -> Result<bool, String> {
    let mut cmd = tool_command(tool);
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
        "GITLAB_TOKEN",
        "GITLAB_ACCESS_TOKEN",
        "OAUTH_TOKEN",
        "CI_JOB_TOKEN",
    ] {
        cmd.env_remove(key);
    }
    cmd.args(["auth", "status", "--hostname", host])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, None) {
        return Ok(answer.is_ok());
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("{tool} isn't installed"),
        _ => format!("couldn't run {tool}: {e}"),
    })?;
    let until = Instant::now() + AUTH_CHECK_FOR;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(false);
            }
        }
    }
}

/// The forge's tool, set to answer this repository in plain JSON.
fn tool_command(tool: &str) -> Command {
    let mut cmd = Command::new(tool);
    cmd.env("GH_PROMPT_DISABLED", "1").env("NO_PROMPT", "1");
    // What it prints is read as JSON: `CLICOLOR_FORCE` or `GH_FORCE_TTY` in the user's
    // environment would colour it even into a pipe, escape codes no JSON reader takes.
    cmd.env_remove("CLICOLOR_FORCE")
        .env_remove("GH_FORCE_TTY")
        .env("NO_COLOR", "1");
    // About the repository it runs in and the host it is given, not one the environment names:
    // `GH_REPO` would answer for another repository, `GH_HOST` send `gh api` (and `-R owner/repo`)
    // to another host than github.com, `GITLAB_HOST` point `glab` at another GitLab than the
    // remote's.
    cmd.env_remove("GH_REPO")
        .env_remove("GH_HOST")
        .env_remove("GITLAB_HOST")
        // glab's own names for the same: the repository, the host, a fork's head project.
        .env_remove("GITLAB_REPO")
        .env_remove("GITLAB_URI")
        .env_remove("GL_HOST")
        .env_remove("GITLAB_HEAD_REPO")
        .env_remove("GITLAB_API_HOST");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// What a tool that failed said, or that it failed: an empty error would read as nothing wrong.
fn failure(tool: &str, out: &std::process::Output) -> String {
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    let stderr = said(&out.stderr);
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = said(&out.stdout);
    if !stdout.is_empty() {
        return stdout;
    }
    format!("{tool} failed ({})", out.status)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-hosted GitLab on a port of its own is reached on it; an SSH port isn't a web one.
    #[test]
    fn gitlab_keeps_the_port_of_its_web_pages() {
        let at = |forge, url| tool_location(forge, url);
        assert_eq!(
            at(Forge::GitLab, "https://git.example.org:8443/team/app.git"),
            Some(("git.example.org:8443".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "ssh://git@git.example.org:2222/team/app.git"),
            Some(("git.example.org".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "git@gitlab.com:team/app.git"),
            Some(("gitlab.com".to_owned(), "team/app".to_owned()))
        );
        // `GH_REPO` takes a plain host.
        assert_eq!(
            at(Forge::GitHub, "https://ghe.example.org:8443/team/app"),
            Some(("ghe.example.org".to_owned(), "team/app".to_owned()))
        );
    }

    /// A repository's remote names a plain project, never another host by way of its path.
    #[test]
    fn only_plain_paths_reach_the_tools() {
        assert!(plain_path("team/app"));
        assert!(plain_path("group/sub/my.app"));
        for bad in [
            "https://evil.example/x/y",
            "git@evil.example:x/y",
            "other.corp/g/p:1",
            "-x/y",
            "app",
            "a//b",
        ] {
            assert!(!plain_path(bad), "{bad}");
        }
        let (_, path) = tool_location(
            Forge::GitLab,
            "https://gitlab.com/https://evil.example/x/y.git",
        )
        .unwrap_or_default();
        assert!(!plain_path(&path), "{path}");
    }

    /// The hosts the arguments send the tool to, which must be the remote's own.
    #[test]
    fn the_hosts_arguments_name_are_found() {
        assert_eq!(
            named_hosts(&["api", "graphql", "--hostname", "ghe.example.org", "-f", "q"]),
            ["ghe.example.org"]
        );
        assert_eq!(
            named_hosts(&["run", "list", "-R", "ghe.example.org/o/r"]),
            ["ghe.example.org"]
        );
        assert!(named_hosts(&["run", "list", "-R", "o/r"]).is_empty());
        assert!(same_host("git.example.org:8443", "git.example.org"));
        assert!(!same_host("github.evil.example", "github.com"));
    }

    /// A tool that hangs is stopped at its time; a long answer still comes whole.
    #[cfg(unix)]
    #[test]
    fn a_tool_given_a_time_is_stopped_at_it() {
        let run_within = |tool: &str, _: &str, here: &Path, args: &[&str], within| {
            let mut cmd = tool_command(tool);
            cmd.current_dir(here).args(args);
            within_command(tool, cmd, |e| e.to_string(), within)
        };
        let here = std::env::temp_dir();
        let started = Instant::now();
        let hung = run_within("sleep", "x", &here, &["20"], Duration::from_millis(300));
        assert!(hung.is_err_and(|e| e.contains("didn't finish within")));
        assert!(started.elapsed() < Duration::from_secs(10));
        let script = "head -c 300000 /dev/zero | tr '\\0' x";
        let long = run_within("sh", "x", &here, &["-c", script], Duration::from_secs(20));
        assert_eq!(long.map(|out| out.len()), Ok(300_000));
        let failed = run_within(
            "sh",
            "x",
            &here,
            &["-c", "echo no >&2; exit 3"],
            Duration::from_secs(20),
        );
        assert_eq!(failed, Err("no".to_owned()));
    }

    /// The user's environment can't colour the JSON read from the tools, nor point them at
    /// another repository or host.
    #[test]
    fn tools_answer_in_plain_json_about_this_repository() {
        for tool in ["gh", "glab"] {
            let cmd = tool_command(tool);
            let envs: HashMap<String, Option<String>> = cmd
                .get_envs()
                .map(|(k, v)| {
                    let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
                    (text(k), v.map(text))
                })
                .collect();
            for removed in [
                "CLICOLOR_FORCE",
                "GH_FORCE_TTY",
                "GH_REPO",
                "GH_HOST",
                "GITLAB_HOST",
                "GITLAB_REPO",
                "GITLAB_URI",
                "GL_HOST",
                "GITLAB_HEAD_REPO",
            ] {
                assert_eq!(envs.get(removed), Some(&None), "{removed}");
            }
            assert_eq!(envs.get("NO_COLOR"), Some(&Some("1".to_owned())));
        }
    }
}

/// What diagnostics say about a forge's tool: its version, and whether it has a login (never
/// any token or account).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolReport {
    pub tool: &'static str,
    /// The first line of `tool --version`; `None` when it isn't installed or didn't answer.
    pub version: Option<String>,
    /// Whether it has a login: `None` when that couldn't be told.
    pub signed_in: Option<bool>,
}

/// How long a tool may take to answer diagnostics.
const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// [`ToolReport`] for `gh` (`auth token` exits 0 when a login is stored, without asking the
/// server; the token itself is never read), `glab` (`auth status`, which asks the server) or
/// `tea` (its configured logins counted).
pub(crate) fn tool_report(tool: &'static str) -> ToolReport {
    let version = report_answer(tool, &["--version"], true).and_then(|(ok, out)| {
        ok.then(|| out.lines().next().unwrap_or_default().trim().to_owned())
            .filter(|v| !v.is_empty())
    });
    let signed_in = match tool {
        _ if version.is_none() => None,
        "tea" => Some(!super::gitea::tea_servers().is_empty()),
        "gh" => report_answer(tool, &["auth", "token"], false).map(|(ok, _)| ok),
        _ => report_answer(tool, &["auth", "status"], false).map(|(ok, _)| ok),
    };
    ToolReport {
        tool,
        version,
        signed_in,
    }
}

/// Runs `tool` with `args` and no input, waiting at most [`REPORT_TIMEOUT`]: whether it
/// succeeded, and its standard output when `keep` (else it goes nowhere). `None` when it
/// couldn't be run or took too long. The output is read as it comes, so a long one can't stall
/// the tool.
fn report_answer(tool: &str, args: &[&str], keep: bool) -> Option<(bool, String)> {
    let mut cmd = tool_command(tool);
    cmd.args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(if keep { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let reader = child.stdout.take().map(|mut out| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut out, &mut text);
            text
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < REPORT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(30));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.and_then(|r| r.join().ok()).unwrap_or_default();
    Some((status.success(), text))
}

/// Recording happens after command preparation, at the process boundary. No forge tool runs.
#[cfg(test)]
pub(super) mod recording {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::process::Command;

    #[derive(Debug)]
    pub struct Call {
        pub tool: String,
        pub args: Vec<String>,
        pub env: Vec<(String, Option<String>)>,
        pub input: Option<String>,
    }
    #[derive(Default)]
    struct Recording {
        calls: Vec<Call>,
        answers: VecDeque<Result<String, String>>,
        remotes: Option<Vec<crate::git::Remote>>,
    }
    thread_local! {
        static ACTIVE: RefCell<Option<Recording>> = const { RefCell::new(None) };
    }
    pub fn run(
        answers: Vec<Result<String, String>>,
        work: impl FnOnce() -> Result<(), String>,
    ) -> Result<Vec<Call>, String> {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                ACTIVE.with(|a| *a.borrow_mut() = None);
            }
        }
        ACTIVE.with(|a| {
            assert!(a.borrow().is_none());
            *a.borrow_mut() = Some(Recording {
                answers: answers.into(),
                ..Default::default()
            });
        });
        let _reset = Reset;
        work()?;
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            let recording = a.take().ok_or("missing active recorder")?;
            assert!(recording.answers.is_empty(), "unconsumed CLI responses");
            Ok(recording.calls)
        })
    }
    pub fn set_remotes(remotes: Vec<crate::git::Remote>) {
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            assert!(a.is_some(), "missing recorder");
            if let Some(r) = a.as_mut() {
                r.remotes = Some(remotes);
            }
        });
    }
    pub(super) fn remotes() -> Option<Vec<crate::git::Remote>> {
        ACTIVE.with(|a| a.borrow().as_ref().and_then(|r| r.remotes.clone()))
    }
    pub(super) fn answer(cmd: &Command, input: Option<&str>) -> Option<Result<String, String>> {
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            let recording = a.as_mut()?;
            recording.calls.push(Call {
                tool: cmd.get_program().to_string_lossy().into_owned(),
                args: cmd
                    .get_args()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
                env: cmd
                    .get_envs()
                    .map(|(k, v)| {
                        (
                            k.to_string_lossy().into_owned(),
                            v.map(|v| v.to_string_lossy().into_owned()),
                        )
                    })
                    .collect(),
                input: input.map(str::to_owned),
            });
            assert!(!recording.answers.is_empty(), "unexpected forge invocation");
            Some(
                recording
                    .answers
                    .pop_front()
                    .unwrap_or_else(|| Err("unexpected forge invocation".into())),
            )
        })
    }
}
