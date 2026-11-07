//! Running commands on a repository's machine, through shells kept open there (`ssh host sh`,
//! `wsl.exe -d distro --exec sh`; see [`super::pool`] and [`super::shell`]), so a command costs
//! one round trip and not a new connection (well over a second for SSH, which can't share one
//! connection between processes on Windows).
//!
//! Each command goes to a shell as a script in braces (which `sh` runs only once it has all of
//! it, so a script cut short runs nothing) that runs it with its output into files of the
//! shell's own temporary folder, then prints a header line (`R <nonce> <exit code> <stdout
//! bytes> <stderr bytes>`) and both outputs as they are: any bytes, any size, no escaping. The
//! nonce, new for every command, makes sure what is read answers that command and no other.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::pool::{Lane, pool};
use super::shell::Shell;
use super::{Host, sh_path, sh_quote};

/// What a shell prints once it is ready, after whatever the login printed.
pub(super) const READY: &str = "__GITCRAFT_READY__";

/// Makes the shell's temporary folder (again, if something cleared it since).
const MAKE_TEMP: &str = "T=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-sh.XXXXXX\")";

/// Where else git may be: a shell that isn't a login one has a short PATH.
const MORE_PATH: &str =
    "PATH=\"$PATH:$HOME/.local/bin:$HOME/bin:/usr/local/bin:/opt/homebrew/bin\"; export PATH";

/// What a command's error output starts with when its folder isn't there.
const NO_FOLDER: &str = "gitcraft: no such folder: ";

/// The largest output a header is believed for (a server that says more is broken).
const MAX_OUTPUT: usize = 1 << 31;

/// A command to run on another machine.
#[derive(Clone, Default)]
pub struct Request {
    /// The folder to run in, as a path there.
    pub cwd: Option<String>,
    /// What to run, as `sh` reads it.
    pub script: String,
    /// Variables set for it (never in a command line: a password stays out of `ps`). Names
    /// that aren't a shell variable's are left out.
    pub env: Vec<(String, String)>,
    /// Variables it must not inherit.
    pub unset: Vec<String>,
    /// What it reads on its input; nothing (`/dev/null`) when `None`.
    pub stdin: Option<Vec<u8>>,
    /// It goes over the network and may take minutes (fetch, push…): run in shells of their
    /// own, so the quick reads aren't left waiting behind it.
    pub long: bool,
    /// How long it may take before it is stopped; `None`: as long as its lane allows (see
    /// [`Request::time_limit`]).
    pub timeout: Option<std::time::Duration>,
}

impl Request {
    /// What it runs, in short, for a message: its script's first line.
    fn what(&self) -> String {
        let first = self.script.lines().next().unwrap_or_default();
        let first = first.strip_prefix("exec ").unwrap_or(first);
        first.chars().take(80).collect()
    }

    /// How long it may run before it is stopped.
    pub(super) fn time_limit(&self) -> Duration {
        self.timeout
            .unwrap_or(if self.long { LONG_FOR } else { QUICK_FOR })
    }
}

/// How long a quick read may take at most (a huge repository's first `status` can take
/// minutes; one that hangs is stopped after this).
const QUICK_FOR: Duration = Duration::from_secs(300);

/// How long a network operation may take at most.
const LONG_FOR: Duration = Duration::from_secs(1800);

/// Without the variables' values (a password) nor the input.
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("Request")
            .field("cwd", &self.cwd)
            .field("script", &self.script)
            .field("env", &names)
            .field("unset", &self.unset)
            .field("stdin", &self.stdin.as_ref().map(Vec::len))
            .field("long", &self.long)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// How a command ended, and what it printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs `req` on `host`, in one of the shells kept open there.
pub fn exec(host: &Host, req: &Request) -> Result<Output, String> {
    exec_mode(host, req, None, &|| true)
}

/// Explicit mutation transport: its caller waits for this actual worker result. A
/// connection loss/deadline remains unknown; it never invokes the legacy stop selector.
/// The caller must retain its owned host receipt/recovery on Unknown and must not retry.
pub(crate) fn exec_owned(
    host: &Host,
    req: &Request,
    owner: &super::OwnedDispatch,
    active: &dyn Fn() -> bool,
) -> Result<Output, super::OwnedFailure> {
    let result = owner.run(|| exec_mode(host, req, Some(owner), active));
    result.map_err(|error| {
        if owner.phase() == super::OwnedPhase::Unknown {
            super::OwnedFailure::Unknown
        } else {
            super::OwnedFailure::NotDispatched(error)
        }
    })
}
fn exec_mode(
    host: &Host,
    req: &Request,
    owned: Option<&super::OwnedDispatch>,
    active: &dyn Fn() -> bool,
) -> Result<Output, String> {
    use super::ownership::{FailureAction, failure_action};
    super::ui_guard::off_ui_thread(&req.what())?;
    if let Some(owner) = owned
        && let Some(error) = owner.context_error()
    {
        owner.not_dispatched();
        return Err(error);
    }
    let lane = if req.long { Lane::Long } else { Lane::Quick };
    let pool = pool(host, lane);
    loop {
        if let Some(owner) = owned
            && (!active() || owner.phase() != super::OwnedPhase::Queued)
        {
            owner.cancel();
            return Err("Owned remote work was cancelled before dispatch".into());
        }
        let mut taken = match pool.take(host, lane) {
            Ok(taken) => taken,
            Err(error) => {
                if let Some(owner) = owned {
                    owner.not_dispatched();
                }
                return Err(error);
            }
        };
        let reused = taken.reused;
        let nonce = nonce();
        let script = framed(req, &nonce);
        let Some(shell) = taken.shell() else {
            continue;
        };
        // This is the dispatch boundary: queue and connection waits have completed,
        // and no byte of this request has yet been sent to the selected shell.
        if let Some(owner) = owned {
            if !active() {
                owner.cancel();
            }
            if !owner.begin() {
                return Err("Owned remote work was cancelled before dispatch".into());
            }
        }
        let limit = req.time_limit();
        match shell.run(&script, &nonce, Instant::now() + limit) {
            Ok(out) => {
                if let Some(owner) = owned {
                    owner.completed();
                }
                taken.give_back();
                return Ok(out);
            }
            Err(failure) => match failure_action(&failure, reused, owned.is_some()) {
                FailureAction::Retry => {} // Legacy reader only; owned sends never retry.
                FailureAction::OwnedUnknown => {
                    if let Some(owner) = owned {
                        owner.unknown();
                    }
                    return Err("Remote mutation outcome is unknown; retain its owned receipt and recovery evidence. No automatic stop, retry or cleanup was dispatched.".into());
                }
                FailureAction::LegacyTimeout(pid) => {
                    drop(taken);
                    if let Some(pid) = pid {
                        stop(host.clone(), pid);
                    }
                    return Err(format!(
                        "{} didn't finish within {} minutes on {} and was stopped",
                        req.what(),
                        limit.as_secs().div_ceil(60),
                        host.label()
                    ));
                }
                FailureAction::LegacyError => return Err(failure.message()),
            },
        }
    }
}

/// Stops the process `pid` on `host` (a command that hung, or the shell it hung in) and
/// everything it started, however deep, in the background: politely, then for good. The
/// processes are listed first, as one stopped leaves what it started to another parent.
fn stop(host: Host, pid: u32) {
    std::thread::spawn(move || {
        let _ = exec(
            &host,
            &Request {
                script: stop_script(pid),
                timeout: Some(Duration::from_secs(30)),
                ..Request::default()
            },
        );
    });
}

/// The script [`stop`] runs: `pid` and its descendants (from `ps`, which every system has with
/// these options), else `pid` alone.
fn stop_script(pid: u32) -> String {
    // Zero denotes a process group to kill(2), and one can be the host init process.
    // Neither can identify an owned timed-out command.
    if pid <= 1 {
        return "true\n".to_owned();
    }
    format!(
        concat!(
            "all=$(ps -A -o pid= -o ppid= 2>/dev/null | awk -v root={pid} '",
            "{{ up[$1] = $2 }} END {{ print root; seen[root] = 1; more = 1; ",
            "while (more) {{ more = 0; for (p in up) if (!(p in seen) && (up[p] in seen)) ",
            "{{ seen[p] = 1; print p; more = 1 }} }} }}')\n",
            "[ -n \"$all\" ] || all={pid}\n",
            "kill -TERM $all 2>/dev/null\n",
            "sleep 2\n",
            "kill -KILL $all 2>/dev/null\n",
            "true\n",
        ),
        pid = pid
    )
}

/// Starts `req` on `host` in a process of its own, whose output the caller reads as it comes
/// (a long `git log`): from the command's first byte on, what a login printed skipped.
pub fn spawn(host: &Host, req: &Request) -> Result<Child, String> {
    super::ui_guard::off_ui_thread(&req.what())?;
    let (mut cmd, attempt) = shell_command(host);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| start_error(host, &e))?;
    let script = format!(
        "{MORE_PATH}\nprintf '\\n%s\\n' {READY}\n{}\n",
        inline(req, "</dev/null")
    );
    let written = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(script.as_bytes()));
    let ready = match (written, child.stdout.as_mut()) {
        (Some(Ok(())), Some(stdout)) => skip_to_ready(stdout),
        _ => false,
    };
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        // What ssh said, as far as it comes in a moment (a proxy can hold the pipe open).
        let said = child
            .stderr
            .take()
            .map(|stderr| super::shell::said_on_ending(stderr, super::shell::SAID_WITHIN))
            .unwrap_or_default();
        return Err(format!("could not reach {}: {said}", host.label()));
    }
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    Ok(child)
}

/// Reads `stdout` up to and with the line [`READY`], a byte at a time (so nothing after it is
/// taken from the caller); `false` when it ended first.
fn skip_to_ready(stdout: &mut std::process::ChildStdout) -> bool {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stdout.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => {
                if line == READY.as_bytes() {
                    return true;
                }
                line.clear();
            }
            Ok(1) => line.push(byte[0]),
            _ => return false,
        }
    }
}

/// A new shell kept open on `host`.
pub(super) fn start_shell(host: &Host) -> Result<Shell, String> {
    let (cmd, attempt) = shell_command(host);
    let shell = Shell::start(cmd, &host.label())?;
    // Signed in: a password to remember is kept now (a failed start drops it).
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    Ok(shell)
}

/// What a new shell is sent first: its temporary folder (removed when the shell ends, also on
/// a hang-up or a signal), its PATH, then [`READY`] and its process (see [`parse_ready`]).
pub(super) fn setup_script() -> String {
    format!(
        concat!(
            "{make} || exit 1\n",
            "{path}\n",
            "trap 'rm -rf \"$T\"' EXIT\n",
            "trap 'rm -rf \"$T\"; exit 1' HUP INT TERM PIPE\n",
            "printf '\\n{ready} %s\\n' \"$$\"\n",
        ),
        make = MAKE_TEMP,
        path = MORE_PATH,
        ready = READY
    )
}

/// The process that runs `sh` on `host`, reading its script from its input, and (over SSH)
/// the attempt to tell once it has signed in.
fn shell_command(host: &Host) -> (Command, Option<super::askpass::Attempt>) {
    let mut attempt = None;
    // Changed further only on Windows (no console window).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = match host {
        Host::Ssh(dest) => {
            let mut cmd = Command::new("ssh");
            // No terminal, no `~.` escapes: the stream is the protocol.
            cmd.args(["-T", "-e", "none"]);
            // Its questions (a password, a passphrase, a new host key) asked in the window
            // (see `askpass`); where that can't be, none asked: it fails, and says why.
            match super::askpass::env_for(&host.label()) {
                Some((env, started)) => {
                    cmd.envs(env);
                    attempt = Some(started);
                }
                None => {
                    cmd.args(["-o", "BatchMode=yes"]);
                }
            }
            // Another ssh config than `~/.ssh/config`, when the user names one.
            if let Some(config) = std::env::var_os("GITCRAFT_SSH_CONFIG").filter(|c| !c.is_empty())
            {
                cmd.arg("-F").arg(config);
            }
            cmd.args([
                "-o",
                "ConnectTimeout=15",
                // A dropped network ends the shell (and its waiting command) within ~45 s.
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "--",
            ]);
            cmd.arg(super::ssh_destination(dest));
            cmd.arg("sh");
            cmd
        }
        Host::Wsl(distro) => {
            let mut cmd = Command::new("wsl.exe");
            cmd.args(["-d", distro, "--exec", "sh"]);
            cmd
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    (cmd, attempt)
}

fn start_error(host: &Host, e: &std::io::Error) -> String {
    let program = match host {
        Host::Ssh(_) => "ssh",
        Host::Wsl(_) => "wsl.exe",
    };
    format!("could not run {program} to reach {}: {e}", host.label())
}

/// A word that names a shell variable (`GIT_EDITOR`), and nothing else.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether a command's error output says its folder isn't there.
pub fn is_missing_folder(stderr: &[u8]) -> bool {
    stderr.starts_with(NO_FOLDER.as_bytes())
}

/// `req` as one `sh` command: in a subshell, so its folder and variables stay its own, with
/// `redirects` applied to it.
fn inline(req: &Request, redirects: &str) -> String {
    let mut s = String::from("(");
    if let Some(cwd) = &req.cwd {
        let path = sh_path(cwd);
        s.push_str(&format!(
            "cd -- {path} 2>/dev/null || {{ printf '{NO_FOLDER}%s\\n' {path} >&2; exit 1; }}\n"
        ));
    }
    for name in req.unset.iter().filter(|n| is_variable_name(n)) {
        s.push_str(&format!("unset {name}\n"));
    }
    for (name, value) in req.env.iter().filter(|(n, _)| is_variable_name(n)) {
        s.push_str(&format!("{name}={}; export {name}\n", sh_quote(value)));
    }
    s.push_str(&req.script);
    s.push_str(&format!("\n) {redirects}"));
    s
}

/// A word no command's output will hold by chance: new for every command.
pub(super) fn nonce() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// `req` as the script a kept-open shell is sent (see the module's doc). The outputs are
/// copied before they are measured: something the command left running (a hook's background
/// job) may still be writing to them, and the bytes sent must be the bytes the header counts.
/// What can't be passed on or kept (a full disk) fails the command (125) and says why.
pub(super) fn framed(req: &Request, nonce: &str) -> String {
    let mut s = format!(
        "{{\nprintf 'S %s\\n' {nonce}\n[ -d \"$T\" ] || {MAKE_TEMP} || exit 1\n: >\"$T/out\"; : >\"$T/err\"\n"
    );
    let (decode, input) = match &req.stdin {
        Some(bytes) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            (
                format!("printf '%s' '{b64}' | base64 -d >\"$T/in\""),
                "\"$T/in\"",
            )
        }
        None => ("true".to_owned(), "/dev/null"),
    };
    s.push_str(&format!("if {decode}; then\n"));
    // In the background, its process said (`J <nonce> <pid>`), so that one that hangs can be
    // stopped (see `exec`).
    s.push_str(&inline(
        req,
        &format!("<{input} >\"$T/out\" 2>\"$T/err\" &"),
    ));
    s.push_str(&format!(
        "\np=$!\nprintf 'J %s %s\\n' {nonce} \"$p\"\nwait \"$p\"; r=$?\n"
    ));
    s.push_str(concat!(
        "else r=125; echo 'gitcraft: could not pass the input on' >\"$T/err\"; fi\n",
        "if cat \"$T/out\" >\"$T/sent-out\" && cat \"$T/err\" >\"$T/sent-err\"; then :; else\n",
        "  r=125; : >\"$T/sent-out\"; echo 'gitcraft: no room for the output in '\"$T\" >\"$T/sent-err\"\n",
        "fi\n",
    ));
    s.push_str(&format!(
        "printf 'R %s %s %s %s\\n' {nonce} \"$r\" \"$(wc -c <\"$T/sent-out\")\" \"$(wc -c <\"$T/sent-err\")\"\n"
    ));
    s.push_str("cat \"$T/sent-out\" \"$T/sent-err\"\nrm -f \"$T/in\" \"$T/out\" \"$T/err\" \"$T/sent-out\" \"$T/sent-err\"\n}\n");
    s
}

/// `<READY> <pid>`, as a new shell says it's ready: `Some` with its process, which is stopped
/// with what it runs if a command in it hangs before its own process is known.
pub(super) fn parse_ready(line: &[u8]) -> Option<Option<u32>> {
    let line = String::from_utf8_lossy(line);
    let mut words = line.split_whitespace();
    if words.next()? != READY {
        return None;
    }
    let pid = words.next().and_then(|p| p.parse().ok());
    (words.next().is_none()).then_some(pid)
}

/// `J <nonce> <pid>`: the process of the command of `nonce`.
pub(super) fn parse_job(line: &str, nonce: &str) -> Option<u32> {
    let mut words = line.split_whitespace();
    if words.next()? != "J" || words.next()? != nonce {
        return None;
    }
    words.next()?.parse().ok()
}

/// `R <nonce> <code> <stdout bytes> <stderr bytes>` (`wc -c` may pad the numbers with
/// spaces), for the command of `nonce` only.
pub(super) fn parse_header(line: &str, nonce: &str) -> Option<(i32, usize, usize)> {
    let mut words = line.split_whitespace();
    if words.next()? != "R" || words.next()? != nonce {
        return None;
    }
    let code = words.next()?.parse().ok()?;
    let out: usize = words.next()?.parse().ok()?;
    let err: usize = words.next()?.parse().ok()?;
    (out <= MAX_OUTPUT && err <= MAX_OUTPUT).then_some((code, out, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_read_with_padded_numbers_for_their_command_only() {
        assert_eq!(parse_header("R ab1 0 12 0\n", "ab1"), Some((0, 12, 0)));
        assert_eq!(
            parse_header("R ab1 128      7       3\n", "ab1"),
            Some((128, 7, 3))
        );
        assert_eq!(parse_header("R ab2 0 1 2\n", "ab1"), None);
        assert_eq!(parse_header("X ab1 0 1 2\n", "ab1"), None);
        assert_eq!(parse_header("R ab1 0 1\n", "ab1"), None);
        // A size no output has.
        assert_eq!(parse_header("R ab1 0 99999999999 0\n", "ab1"), None);
    }

    #[test]
    fn a_request_sets_its_folder_and_variables_in_a_subshell() {
        let req = Request {
            cwd: Some("~/my repo".to_owned()),
            script: "exec git status".to_owned(),
            env: vec![
                ("A".to_owned(), "it's".to_owned()),
                ("X;rm -rf ~".to_owned(), "1".to_owned()),
            ],
            unset: vec!["B".to_owned(), "$(x)".to_owned()],
            ..Request::default()
        };
        let s = inline(&req, "</dev/null");
        assert_eq!(
            s,
            "(cd -- \"$HOME\"'/my repo' 2>/dev/null || { printf 'gitcraft: no such folder: %s\\n' \"$HOME\"'/my repo' >&2; exit 1; }\nunset B\nA='it'\\''s'; export A\nexec git status\n) </dev/null"
        );
    }

    /// Nothing runs before the shell has the whole script: it is one group in braces.
    #[test]
    fn a_framed_script_is_one_group() {
        let s = framed(&Request::default(), "n1");
        assert!(s.starts_with("{\n") && s.ends_with("\n}\n"), "{s}");
    }

    /// No two files of the framing differ only in case: on macOS (and Windows) they would be
    /// one file, and copying one onto the other would empty it.
    #[test]
    fn framing_files_differ_beyond_case() {
        let s = framed(
            &Request {
                stdin: Some(b"x".to_vec()),
                ..Request::default()
            },
            "n1",
        );
        let mut names: Vec<String> = s
            .split("$T/")
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect::<String>()
            })
            .collect();
        names.sort();
        names.dedup();
        let mut folded: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        folded.sort();
        folded.dedup();
        assert_eq!(names.len(), folded.len(), "{names:?}");
        assert!(names.len() >= 5, "{names:?}");
    }

    #[test]
    fn a_ready_shell_says_its_process() {
        assert_eq!(parse_ready(b"__GITCRAFT_READY__ 4242\n"), Some(Some(4242)));
        assert_eq!(parse_ready(b"__GITCRAFT_READY__\n"), Some(None));
        assert_eq!(parse_ready(b"login banner\n"), None);
        assert_eq!(parse_ready(b"__GITCRAFT_READY__ 1 2\n"), None);
    }

    /// Run the exact production selector only over synthetic data and recording sinks.
    /// This contains the test; it does not establish ownership of production host PIDs.
    #[cfg(unix)]
    #[test]
    fn stopping_ends_a_whole_tree() -> Result<(), String> {
        const SNAPSHOT: &str = concat!(
            "1 0\n",
            "10 1\n",
            "20 10\n",
            "100 20\n",
            "101 100\n",
            "102 101\n",
            "103 102\n",
            "110 20\n",
            "200 1\n",
            "0 0\n",
            "300 301\n",
            "301 300\n",
        );
        // A safety allowlist checked BEFORE starting the shell. Any new command,
        // absolute/builtin escape or awk extension requires a separate static review.
        // Semantic assertions below still exercise the actual production awk program.
        const ALLOWED: &str = concat!(
            "all=$(ps -A -o pid= -o ppid= 2>/dev/null | awk -v root=@PID@ '",
            "{ up[$1] = $2 } END { print root; seen[root] = 1; more = 1; ",
            "while (more) { more = 0; for (p in up) if (!(p in seen) && (up[p] in seen)) ",
            "{ seen[p] = 1; print p; more = 1 } } }')\n",
            "[ -n \"$all\" ] || all=@PID@\n",
            "kill -TERM $all 2>/dev/null\n",
            "sleep 2\n",
            "kill -KILL $all 2>/dev/null\n",
            "true\n",
        );
        let reversed = SNAPSHOT.lines().rev().collect::<Vec<_>>().join("\n") + "\n";
        for (pid, snapshot, expected) in [
            (100, SNAPSHOT, vec![100, 101, 102, 103]),
            (100, reversed.as_str(), vec![100, 101, 102, 103]),
            // Empty synthetic lookup retains the requested-root fallback; it proves no
            // authority to signal that PID outside this inert fixture.
            (100, "", vec![100]),
            (0, SNAPSHOT, vec![]),
            (1, SNAPSHOT, vec![]),
        ] {
            let script = stop_script(pid);
            let allowed = if pid <= 1 {
                "true\n".to_owned()
            } else {
                ALLOWED.replace("@PID@", &pid.to_string())
            };
            assert_eq!(
                script, allowed,
                "Unreviewed stop-script change: do not execute it"
            );
            let fixture = format!(
                concat!(
                    "exec 3>&1\n",
                    "fixture_snapshot={}\n",
                    "ps() {{ printf 'PS' >&3; printf ' %s' \"$@\" >&3; printf '\\n' >&3; printf '%s' \"$fixture_snapshot\"; }}\n",
                    "kill() {{ printf 'SIGNAL' >&3; printf ' %s' \"$@\" >&3; printf '\\n' >&3; }}\n",
                    "sleep() {{ printf 'SLEEP' >&3; printf ' %s' \"$@\" >&3; printf '\\n' >&3; }}\n",
                    "{}"
                ),
                sh_quote(snapshot),
                script,
            );
            // ps's event must bypass command substitution, while its fixed numeric
            // snapshot alone goes to the real awk. Functions shadow every signal/sleep.
            let output = Command::new("/bin/sh")
                .args(["-c", &fixture])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .current_dir(std::env::temp_dir())
                .stdin(Stdio::null())
                .output()
                .map_err(|e| e.to_string())?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let events = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
            if expected.is_empty() {
                assert!(events.is_empty(), "{events}");
                continue;
            }
            let lines = events.lines().collect::<Vec<_>>();
            assert_eq!(lines.len(), 4, "{events}");
            assert_eq!(lines[0], "PS -A -o pid= -o ppid=");
            assert_eq!(lines[2], "SLEEP 2");
            let expected = expected
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            for (line, signal) in [(lines[1], "-TERM"), (lines[3], "-KILL")] {
                let words = line.split_whitespace().collect::<Vec<_>>();
                assert_eq!(words.get(..2), Some(["SIGNAL", signal].as_slice()));
                let selected = words[2..]
                    .iter()
                    .map(|p| p.parse::<u32>().map_err(|e| e.to_string()))
                    .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
                assert_eq!(selected, expected, "{events}");
                for foreign in [0, 1, 10, 20, 110, 200, 300, 301] {
                    assert!(!selected.contains(&foreign), "{events}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_request_shows_no_values() {
        let req = Request {
            env: vec![("PASSWORD".to_owned(), "hunter2".to_owned())],
            ..Request::default()
        };
        let shown = format!("{req:?}");
        assert!(
            shown.contains("PASSWORD") && !shown.contains("hunter2"),
            "{shown}"
        );
    }
}
