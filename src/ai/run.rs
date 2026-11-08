//! Running an AI tool on the repository's machine: here, or in WSL / over SSH through the
//! shells kept open there (see [`crate::remote`]). The prompt goes on the tool's input, never
//! into its command line; what it prints is the answer. It can be stopped (with everything it
//! started), and is stopped when it takes too long.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::settings::{AiSettings, AiTool};
use crate::remote::{self, Target};

/// How long a tool may take to answer before it is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// The most of an answer kept.
const MAX_ANSWER: usize = 1 << 20;

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
                    if w.contains([' ', '&', '|', '<', '>', '^', '(', ')']) {
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
    cmd.current_dir(job.repo).env("NO_COLOR", "1");
    crate::shell_path::apply(&mut cmd);
    run_owned(ToolCommand::Here(cmd), job)
}

enum ToolCommand {
    Here(Command),
    There(remote::Streaming),
}
fn run_owned(command: ToolCommand, job: &Job<'_>) -> Result<Ran, RunError> {
    let cancel = job.cancel.clone();
    let mut options = crate::processes::Options {
        input: Some(job.input.to_vec()),
        background: true,
        timeout: Some(job.timeout),
        cancelled: Some(Box::new(move || cancel.is_cancelled())),
        output_limit: MAX_ANSWER,
        stderr_limit: Some(MAX_ANSWER),
        drain_for: Duration::from_secs(2),
        ..Default::default()
    };
    let command = match command {
        ToolCommand::Here(command) => command,
        ToolCommand::There(streaming) => streaming.configure(&mut options),
    };
    owned_completion(crate::processes::run_checked(command, options), job)
}
fn owned_completion(
    result: Result<crate::processes::Output, crate::processes::Failure>,
    job: &Job<'_>,
) -> Result<Ran, RunError> {
    match result {
        Err(crate::processes::Failure::CancelledBeforeStart) => Err(RunError::Cancelled),
        Err(crate::processes::Failure::TimedOutBeforeStart) => {
            Err(timed_out(&job.command.program(), job.timeout))
        }
        Err(e) => Err(RunError::Failed(e.message())),
        Ok(out) => {
            if let Some(reason) = out.stopped {
                if out.stop_kind == Some(crate::processes::StopKind::Cancelled) {
                    return Err(RunError::Cancelled);
                }
                if out.stop_kind == Some(crate::processes::StopKind::TimedOut) {
                    return Err(timed_out(&job.command.program(), job.timeout));
                }
                return Err(RunError::Failed(reason));
            }
            Ok(Ran {
                code: out.status.code(),
                stdout: out.stdout,
                stderr: out.stderr,
            })
        }
    }
}

/// The shell that runs `command` here.
fn shell_here(command: &CommandLine) -> Command {
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

/// The tool runs apart from pooled Git requests on bounded process-service capacity.
/// Its prompt stays on stdin; the supervisor's controller channel is separate.
fn run_there(target: &Target, job: &Job<'_>) -> Result<Ran, RunError> {
    let req = remote::Request {
        cwd: Some(target.path.clone()),
        script: job.command.sh(),
        env: vec![("NO_COLOR".into(), "1".into())],
        stdin: Some(job.input.to_vec()),
        apart: true,
        timeout: Some(job.timeout),
        ..Default::default()
    };
    let streaming = remote::streaming_command(&target.host, &req).map_err(RunError::Failed)?;
    run_owned(ToolCommand::There(streaming), job)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_cancel_keeps_native_failures_and_typed_uncertainty() {
        use crate::processes::{Failure, StopKind, recording_output};
        let cancel = Cancel::default();
        cancel.cancel();
        let command = CommandLine::Line("never executed".into());
        let job = Job {
            repo: Path::new("/recording-only"),
            command: &command,
            input: b"",
            timeout: Duration::from_secs(1),
            cancel: &cancel,
        };
        for failure in [
            "could not finish owned process",
            "Process service stopped before returning a result",
        ] {
            let error = owned_completion(Err(Failure::Failed(failure.into())), &job)
                .err()
                .map(|e| e.message())
                .unwrap_or_default();
            assert!(error.contains(failure));
        }
        let error = owned_completion(Ok(recording_output(Some(StopKind::Uncertain))), &job)
            .err()
            .map(|e| e.message())
            .unwrap_or_default();
        assert!(error.contains("cleanup proof missing"));
        assert!(matches!(
            owned_completion(Ok(recording_output(Some(StopKind::Cancelled))), &job),
            Err(RunError::Cancelled)
        ));
        assert!(matches!(
            owned_completion(Err(Failure::CancelledBeforeStart), &job),
            Err(RunError::Cancelled)
        ));
    }

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

    /// Exercise prompt/output semantics through the actual owned loopback dispatcher.
    #[cfg(unix)]
    #[test]
    fn the_script_for_another_machine_passes_the_input_on() -> Result<(), String> {
        let repo = std::env::temp_dir();
        let target = Target {
            host: remote::Host::Ssh(remote::LOOPBACK.into()),
            path: repo.to_string_lossy().into_owned(),
        };
        let command = CommandLine::Line("cat; printf err >&2; exit 3".into());
        let cancel = Cancel::default();
        let job = Job {
            repo: &repo,
            command: &command,
            input: b"prompt quoted \n",
            cancel: &cancel,
            timeout: Duration::from_secs(10),
        };
        let ran = run_there(&target, &job).map_err(|e| e.message())?;
        assert_eq!(ran.code, Some(3));
        assert_eq!(ran.stdout, b"prompt quoted \n");
        assert_eq!(ran.stderr, b"err");
        Ok(())
    }
}
