//! Running an AI tool on the repository's machine: here, or in WSL / over SSH through the
//! shells kept open there (see [`crate::remote`]). The prompt goes on the tool's input, never
//! into its command line; what it prints is the answer. It can be stopped (with everything it
//! started), and is stopped when it takes too long.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::settings::{AiSettings, AiTool};
use crate::remote::{self, Target};

/// How long a tool may take to answer before it is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// The most of an answer kept.
const MAX_ANSWER: usize = 1 << 20;

/// How often a running tool is checked on (finished, stopped, out of time).
const TICK: Duration = Duration::from_millis(50);

/// Asks a running tool to stop. Clones share it.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandLine {
    /// A program and its arguments, each passed as it is.
    Words(Vec<String>),
    /// A command line of the user's, as their shell reads it (`sh`, or `cmd` here on Windows).
    Line(String),
    /// A script of gitcraft's, written for each shell.
    Script { sh: String, cmd: String },
}

/// The instruction `gh copilot` is given on its command line; the prompt itself comes on its
/// input like every other tool's.
const COPILOT_INSTRUCTION: &str = "Follow the instructions given on standard input.";

impl CommandLine {
    /// What runs the tool the settings pick.
    pub fn for_tool(settings: &AiSettings) -> Result<Self, String> {
        let words = |w: &[&str]| CommandLine::Words(w.iter().map(|s| (*s).to_owned()).collect());
        match settings.tool {
            AiTool::Off => Err("Pick a tool in Settings → AI first.".to_owned()),
            AiTool::Claude => Ok(words(&["claude", "-p", "--output-format", "text"])),
            AiTool::Codex => Ok(words(&[
                "codex",
                "exec",
                "--color",
                "never",
                "--sandbox",
                "read-only",
                "-",
            ])),
            AiTool::Copilot => Ok(words(&["gh", "copilot", "-p", COPILOT_INSTRUCTION, "-s"])),
            AiTool::Custom => {
                let line = settings.custom_command.trim();
                if line.is_empty() {
                    Err("The custom command is empty: set it in Settings → AI.".to_owned())
                } else {
                    Ok(CommandLine::Line(line.to_owned()))
                }
            }
        }
    }

    /// As `sh` runs it.
    pub fn sh(&self) -> String {
        match self {
            CommandLine::Words(words) => words
                .iter()
                .map(|w| remote::sh_quote(w))
                .collect::<Vec<_>>()
                .join(" "),
            CommandLine::Line(line) => line.clone(),
            CommandLine::Script { sh, .. } => sh.clone(),
        }
    }

    /// As `cmd` runs it (Windows). The words are gitcraft's own, none with a `"` in it.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn cmd(&self) -> String {
        match self {
            CommandLine::Words(words) => words
                .iter()
                .map(|w| {
                    if w.contains([' ', '&', '|', '<', '>', '^', '(', ')', '=', ',', ';', '\'']) {
                        format!("\"{w}\"")
                    } else {
                        w.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            CommandLine::Line(line) => line.clone(),
            CommandLine::Script { cmd, .. } => cmd.clone(),
        }
    }

    /// The program, for a message: "claude", "gh".
    pub fn program(&self) -> String {
        match self {
            CommandLine::Words(words) => words.first().cloned().unwrap_or_default(),
            CommandLine::Line(line) => first_word(line),
            CommandLine::Script { .. } => "sh".to_owned(),
        }
    }
}

/// The first word of a command line, its quotes taken off: `'/my tools/ask' -x` → `/my tools/ask`.
fn first_word(line: &str) -> String {
    let line = line.trim_start();
    match line.chars().next() {
        Some(q @ ('\'' | '"')) => line[1..].split(q).next().unwrap_or_default().to_owned(),
        _ => line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned(),
    }
}

/// Why there is no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// Stopped on request.
    Cancelled,
    /// What went wrong, as the user reads it.
    Failed(String),
}

impl RunError {
    pub fn message(&self) -> String {
        match self {
            RunError::Cancelled => "Stopped.".to_owned(),
            RunError::Failed(e) => e.clone(),
        }
    }
}

/// One run: what, where, with what on its input.
pub struct Job<'a> {
    /// The repository: the tool runs in its folder, on its machine.
    pub repo: &'a Path,
    pub command: &'a CommandLine,
    pub input: &'a [u8],
    pub timeout: Duration,
    pub cancel: &'a Cancel,
}

/// How a tool ended.
struct Ran {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Runs `job` and returns what the tool printed. Blocking: from a background thread.
pub fn run(job: &Job<'_>) -> Result<String, RunError> {
    let place = remote::parse(job.repo);
    let ran = match &place {
        Some(target) => run_there(target, job)?,
        None => run_here(job)?,
    };
    if job.cancel.is_cancelled() {
        return Err(RunError::Cancelled);
    }
    let where_ = place.map_or_else(|| "this machine".to_owned(), |t| t.host.label());
    outcome(&job.command.program(), &where_, ran)
}

/// The answer of a tool that ended so, or what to say of its failure.
fn outcome(program: &str, where_: &str, ran: Ran) -> Result<String, RunError> {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    match ran.code {
        Some(0) => Ok(text(&ran.stdout)),
        // `sh`'s "not found", and `cmd`'s.
        Some(127 | 9009) => Err(RunError::Failed(format!(
            "{program} isn't installed on {where_} (or isn't on the PATH gitcraft's commands get \
             there). Install it, or set a custom command with its full path in Settings → AI."
        ))),
        code => {
            let said =
                last_lines(&text(&ran.stderr), 15).or_else(|| last_lines(&text(&ran.stdout), 15));
            let how = code.map_or_else(
                || "was stopped by a signal".to_owned(),
                |c| format!("exited with code {c}"),
            );
            Err(RunError::Failed(match said {
                Some(said) => format!("{program} {how}: {said}"),
                None => format!("{program} {how} and said nothing."),
            }))
        }
    }
}

/// The last `n` lines of `s` that aren't blank, colours taken out; `None` when there are none.
fn last_lines(s: &str, n: usize) -> Option<String> {
    let clean = super::prompt::clean_answer(s);
    let lines: Vec<&str> = clean.lines().filter(|l| !l.trim().is_empty()).collect();
    let from = lines.len().saturating_sub(n);
    (!lines.is_empty()).then(|| lines[from..].join("\n"))
}

/// What a tool that ran out of time is said to have done.
fn timed_out(program: &str, timeout: Duration) -> RunError {
    let secs = (timeout.as_millis().div_ceil(1000) as u64).max(1);
    let took = match secs {
        1 => "a second".to_owned(),
        s if s >= 120 && s % 60 == 0 => format!("{} minutes", s / 60),
        s => format!("{s} seconds"),
    };
    RunError::Failed(format!(
        "{program} didn't answer within {took} and was stopped."
    ))
}

/// The tool, run here: by `sh` (by `cmd` on Windows), in its own process group so that
/// stopping it stops what it started too.
fn run_here(job: &Job<'_>) -> Result<Ran, RunError> {
    let mut cmd = shell_here(job.command);
    cmd.current_dir(job.repo)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::shell_path::apply(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| {
        RunError::Failed(format!(
            "Couldn't start {} in {}: {e}",
            job.command.program(),
            job.repo.display()
        ))
    })?;
    let input = job.input.to_vec();
    let writer = child.stdin.take().map(|mut stdin| {
        // A tool that stops reading has stopped for a reason, which its exit code and error
        // output tell better than the broken pipe would.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        })
    });
    let stdout = child.stdout.take().map(read_capped);
    let stderr = child.stderr.take().map(read_capped);
    let started = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(e) => {
                stop_here(&mut child);
                return Err(RunError::Failed(format!("Lost track of the tool: {e}")));
            }
        }
        if job.cancel.is_cancelled() {
            stop_here(&mut child);
            return Err(RunError::Cancelled);
        }
        if started.elapsed() >= job.timeout {
            stop_here(&mut child);
            return Err(timed_out(&job.command.program(), job.timeout));
        }
        std::thread::sleep(TICK);
    };
    // Something the tool left running may hold its output open: what came by then is the
    // answer, and what is still running is stopped.
    let mut out = [(stdout, None), (stderr, None)];
    let until = Instant::now() + LINGER;
    for (reader, got) in &mut out {
        *got = collect(reader.as_ref(), until);
    }
    if out.iter().any(|(_, got)| got.is_none()) {
        // Stopping it closes the pipes, and what was read comes.
        stop_here(&mut child);
        let until = Instant::now() + LINGER;
        for (reader, got) in &mut out {
            if got.is_none() {
                *got = collect(reader.as_ref(), until);
            }
        }
    }
    drop(writer);
    let [(_, stdout), (_, stderr)] = out;
    Ok(Ran {
        code,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

/// How long a pipe may stay open after the tool ended.
const LINGER: Duration = Duration::from_secs(2);

/// What a reader got, once its pipe closed before `until`; `None` if it didn't.
fn collect(reader: Option<&Reader>, until: Instant) -> Option<Vec<u8>> {
    let Some(reader) = reader else {
        return Some(Vec::new());
    };
    reader
        .recv_timeout(until.saturating_duration_since(Instant::now()))
        .ok()
}

/// The shell that runs `command` here.
pub(crate) fn shell_here(command: &CommandLine) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        // `/S`: the quotes around the line are taken off and the rest kept as it is.
        cmd.raw_arg(format!("/D /S /C \"{}\"", command.cmd()));
        // cmd looks for a program in the folder it runs in before the PATH: a repository's
        // own `claude.cmd` would run in place of the tool.
        cmd.env("NoDefaultCurrentDirectoryInExePath", "1");
        cmd.creation_flags(0x0800_0000);
        cmd
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command.sh());
        cmd.process_group(0);
        cmd
    }
}

/// What a pipe's reader sends once the pipe closes.
type Reader = std::sync::mpsc::Receiver<Vec<u8>>;

/// Reads all of `pipe` in a thread of its own, keeping at most [`MAX_ANSWER`] bytes, and sends
/// it once the pipe closes.
fn read_capped(mut pipe: impl Read + Send + 'static) -> Reader {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = MAX_ANSWER.saturating_sub(kept.len());
                    kept.extend_from_slice(&buf[..n.min(room)]);
                }
            }
        }
        let _ = tx.send(kept);
    });
    rx
}

/// Stops a tool run here and everything it started: politely, then for good.
pub(crate) fn stop_here(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid])
            .creation_flags(0x0800_0000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let group = format!("-{pid}");
        let signal = |sig: &str| {
            let _ = Command::new("kill")
                .args([sig, "--", &group])
                .stderr(Stdio::null())
                .status();
        };
        signal("-TERM");
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until && matches!(child.try_wait(), Ok(None)) {
            std::thread::sleep(TICK);
        }
        signal("-KILL");
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// A name for one run on another machine, for its process's file there.
fn run_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{nanos:x}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Where a run on another machine writes its process, for stopping it.
fn pid_file(id: &str) -> String {
    format!("\"${{TMPDIR:-/tmp}}/gitcraft-ai-{id}.pid\"")
}

/// The script that runs `command` on another machine: in a process of its own, written to
/// [`pid_file`] while it runs, its input passed on.
fn there_script(command: &CommandLine, id: &str) -> String {
    format!(
        concat!(
            "p={pid}\n",
            "trap 'rm -f \"$p\"' EXIT\n",
            "trap 'rm -f \"$p\"; exit 143' HUP INT TERM\n",
            "exec 3<&0\n",
            "sh -c {cmd} <&3 3<&- &\n",
            "c=$!\n",
            "echo \"$c\" >\"$p\"\n",
            "wait \"$c\"; r=$?\n",
            "rm -f \"$p\"\n",
            "exit \"$r\"\n",
        ),
        pid = pid_file(id),
        cmd = remote::sh_quote(&command.sh()),
    )
}

/// The tool, run on `target`'s machine in its folder; stopped there when cancelled.
fn run_there(target: &Target, job: &Job<'_>) -> Result<Ran, RunError> {
    let id = run_id();
    let req = remote::Request {
        cwd: Some(target.path.clone()),
        script: there_script(job.command, &id),
        env: vec![("NO_COLOR".to_owned(), "1".to_owned())],
        stdin: Some(job.input.to_vec()),
        // Minutes, maybe: in shells apart from git's, so neither its reads nor a fetch or push
        // wait behind it.
        apart: true,
        timeout: Some(job.timeout),
        ..remote::Request::default()
    };
    let done = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (done, cancel, host) = (done.clone(), job.cancel.clone(), target.host.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                if cancel.is_cancelled() {
                    stop_there(&host, &id, &done);
                    return;
                }
                std::thread::sleep(TICK * 2);
            }
        })
    };
    let started = Instant::now();
    let out = remote::exec(&target.host, &req);
    done.store(true, Ordering::SeqCst);
    let _ = watcher.join();
    match out {
        Ok(out) => Ok(Ran {
            code: Some(out.code),
            stdout: out.stdout,
            stderr: out.stderr,
        }),
        Err(_) if job.cancel.is_cancelled() => Err(RunError::Cancelled),
        Err(_) if started.elapsed() >= job.timeout => {
            Err(timed_out(&job.command.program(), job.timeout))
        }
        Err(e) => Err(RunError::Failed(e)),
    }
}

/// Stops the run `id` on `host` and what it started, once it has said its process: it may be
/// waiting for a shell or its input may still be on the way, so this asks until it has, or
/// until the run is `done`.
fn stop_there(host: &remote::Host, id: &str, done: &AtomicBool) {
    let read = remote::Request {
        script: format!("cat {} 2>/dev/null", pid_file(id)),
        timeout: Some(Duration::from_secs(30)),
        ..remote::Request::default()
    };
    while !done.load(Ordering::SeqCst) {
        let pid = remote::exec(host, &read)
            .ok()
            .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse().ok());
        if let Some(pid) = pid {
            remote::stop_process(host, pid);
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tool_has_its_command() -> Result<(), String> {
        let line = |tool, custom: &str| {
            CommandLine::for_tool(&AiSettings {
                tool,
                custom_command: custom.to_owned(),
                ..AiSettings::default()
            })
        };
        assert_eq!(
            line(AiTool::Claude, "")?.sh(),
            "'claude' '-p' '--output-format' 'text'"
        );
        assert!(line(AiTool::Codex, "")?.sh().ends_with("'-'"));
        let copilot = line(AiTool::Copilot, "")?;
        assert_eq!(copilot.program(), "gh");
        assert!(
            copilot
                .cmd()
                .contains("\"Follow the instructions given on standard input.\"")
        );
        assert_eq!(
            line(AiTool::Custom, "  ollama run llama3 ")?,
            CommandLine::Line("ollama run llama3".to_owned())
        );
        assert!(line(AiTool::Custom, " ").is_err());
        assert_eq!(
            CommandLine::Line("'/my tools/ask' -x".to_owned()).program(),
            "/my tools/ask"
        );
        assert_eq!(CommandLine::Line("llm -m x".to_owned()).program(), "llm");
        assert!(line(AiTool::Off, "x").is_err());
        Ok(())
    }

    #[test]
    fn failures_say_what_the_tool_said() {
        let ran = |code, out: &str, err: &str| Ran {
            code,
            stdout: out.as_bytes().to_vec(),
            stderr: err.as_bytes().to_vec(),
        };
        assert_eq!(
            outcome("x", "here", ran(Some(0), "ok\n", "noise")),
            Ok("ok\n".to_owned())
        );
        let missing = outcome("claude", "SSH · box", ran(Some(127), "", "not found"));
        assert!(
            matches!(&missing, Err(RunError::Failed(e)) if e.starts_with("claude isn't installed on SSH · box")),
            "{missing:?}"
        );
        let failed = outcome(
            "codex",
            "here",
            ran(Some(2), "", "\u{1b}[31mnot logged in\u{1b}[0m\n"),
        );
        assert_eq!(
            failed,
            Err(RunError::Failed(
                "codex exited with code 2: not logged in".to_owned()
            ))
        );
        let quiet = outcome("gh", "here", ran(Some(1), "", ""));
        assert_eq!(
            quiet,
            Err(RunError::Failed(
                "gh exited with code 1 and said nothing.".to_owned()
            ))
        );
        let signal = outcome("gh", "here", ran(None, "partial", ""));
        assert_eq!(
            signal,
            Err(RunError::Failed(
                "gh was stopped by a signal: partial".to_owned()
            ))
        );
    }

    #[test]
    fn a_timeout_is_said_in_minutes_or_seconds() {
        assert_eq!(
            timed_out("claude", TIMEOUT).message(),
            "claude didn't answer within 5 minutes and was stopped."
        );
        assert_eq!(
            timed_out("x", Duration::from_millis(300)).message(),
            "x didn't answer within a second and was stopped."
        );
        assert_eq!(
            timed_out("x", Duration::from_secs(90)).message(),
            "x didn't answer within 90 seconds and was stopped."
        );
    }

    /// Stopped on that machine (a timeout, a cancel), the script leaves no file behind.
    #[cfg(unix)]
    #[test]
    fn the_script_for_another_machine_cleans_up_when_stopped() -> Result<(), String> {
        let id = run_id();
        let command = CommandLine::Line("sleep 30".to_owned());
        let mut child = Command::new("sh")
            .args(["-c", &there_script(&command, &id)])
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_owned());
        let file = Path::new(&tmp).join(format!("gitcraft-ai-{id}.pid"));
        let until = Instant::now() + Duration::from_secs(5);
        while !file.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let written = file.exists();
        let _ = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status();
        let _ = child.wait();
        let left = file.exists();
        assert!(written && !left);
        Ok(())
    }

    /// The script for another machine, run here by `sh` the same way: the input reaches the
    /// tool, its output and exit code come back, and its process is written down while it
    /// runs and removed after.
    #[cfg(unix)]
    #[test]
    fn the_script_for_another_machine_passes_the_input_on() -> Result<(), String> {
        let id = run_id();
        let command = CommandLine::Line(
            "cat; echo \"pid=$(cat \"${TMPDIR:-/tmp}/gitcraft-ai-ID.pid\")\" >&2; exit 3"
                .replace("ID", &id),
        );
        let mut child = Command::new("sh")
            .args(["-c", &there_script(&command, &id)])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all("prompt ü 'quoted' $HOME\n".as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_owned());
        let left = Path::new(&tmp)
            .join(format!("gitcraft-ai-{id}.pid"))
            .exists();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "prompt ü 'quoted' $HOME\n"
        );
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.trim()
                .strip_prefix("pid=")
                .is_some_and(|p| p.parse::<u32>().is_ok()),
            "{said}"
        );
        assert!(!left);
        Ok(())
    }
}
