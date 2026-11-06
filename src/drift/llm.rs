//! The optional second opinion: an installed coding agent's command line (`claude -p`, or
//! `codex exec`) asked which of the changed files don't fit the task. It runs here, with the
//! user's own login, and is given only the task and the figures (paths, no file contents).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Report;

/// How long the answer may take.
const TIMEOUT: Duration = Duration::from_secs(180);

/// Most file paths put in the question.
const MAX_FILES: usize = 300;

/// The tools tried, in order: the program, and the arguments that make it answer one prompt
/// read from its input and quit.
/// Codex runs in a folder that isn't a repository (it is given no files), which it refuses
/// unless told.
const TOOLS: [(&str, &[&str]); 2] = [
    ("claude", &["-p"]),
    ("codex", &["exec", "--skip-git-repo-check", "-"]),
];

/// The question asked.
pub fn prompt(task: &str, report: &Report) -> String {
    let mut files: Vec<&str> = report
        .files
        .iter()
        .map(String::as_str)
        .take(MAX_FILES)
        .collect();
    if report.files.len() > MAX_FILES {
        files.push("…");
    }
    let deps: Vec<String> = report
        .dependencies
        .iter()
        .map(|d| format!("{} ({})", d.name, d.manifest))
        .collect();
    let sigs: Vec<String> = report
        .signatures
        .iter()
        .take(50)
        .map(|s| format!("{}:{}: {}", s.path, s.line, s.text))
        .collect();
    format!(
        "A coding agent was given this task:\n\n{task}\n\nIt changed these files:\n{}\n\n\
         It added these dependencies: {}\nIt changed these public signatures:\n{}\n\n\
         Which of the changed files, dependencies and signatures are not needed for the task? \
         Answer with one line per item: the path or name, a dash, and a short reason. Answer \
         NONE if everything fits. Do not run any tools and do not change anything.",
        files.join("\n"),
        if deps.is_empty() {
            "none".to_owned()
        } else {
            deps.join(", ")
        },
        if sigs.is_empty() {
            "none".to_owned()
        } else {
            sigs.join("\n")
        },
    )
}

/// Asks the first installed tool; its answer, or why there is none.
pub fn ask(question: &str) -> Result<String, String> {
    let mut missing = Vec::new();
    let mut failed = Vec::new();
    for (tool, args) in TOOLS {
        match run(tool, args, question) {
            Err(Missing) => missing.push(tool),
            Ok(Ok(answer)) => return Ok(format!("{tool}: {}", answer.trim())),
            // Installed but not answering (not signed in…): the next one is tried.
            Ok(Err(e)) => failed.push(e),
        }
    }
    if failed.is_empty() {
        Err(format!(
            "No coding agent's command line is installed ({}): the figures above are the exact part",
            missing.join(", ")
        ))
    } else {
        Err(failed.join("\n"))
    }
}

/// The tool isn't installed.
struct Missing;

fn run(tool: &str, args: &[&str], question: &str) -> Result<Result<String, String>, Missing> {
    match start(tool, args) {
        Err(Missing) if cfg!(windows) => {
            // Installed with npm, it is a `.cmd`, which only a shell finds by its bare name.
            finish(tool, start(&format!("{tool}.cmd"), args)?, question)
        }
        started => finish(tool, started?, question),
    }
}

fn start(tool: &str, args: &[&str]) -> Result<std::io::Result<std::process::Child>, Missing> {
    let mut cmd = Command::new(tool);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(std::env::temp_dir());
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.spawn() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Missing),
        other => Ok(other),
    }
}

fn finish(
    tool: &str,
    child: std::io::Result<std::process::Child>,
    question: &str,
) -> Result<Result<String, String>, Missing> {
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Ok(Err(format!("Could not run {tool}: {e}"))),
    };
    // Written on a thread of its own: a tool that doesn't read it (or reads it slowly) can't
    // hold the timeout off.
    if let Some(mut stdin) = child.stdin.take() {
        let question = question.to_owned();
        std::thread::spawn(move || {
            let _ = stdin.write_all(question.as_bytes());
        });
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut out);
            }
            out
        })
    };
    let out = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(Err(format!(
                    "{tool} took longer than {} s",
                    TIMEOUT.as_secs()
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Ok(Err(format!("Could not wait for {tool}: {e}"))),
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    Ok(if status.success() {
        Ok(out)
    } else {
        Err(format!("{tool} failed: {}", err.trim()))
    })
}
