//! Running a project's own tool (`cargo`, `npm`) in the repository, on whichever machine it is:
//! here, or in WSL or over SSH through the shells kept open there.

use std::path::Path;
use std::time::Duration;

/// How long a listing may take.
const TIME: Duration = Duration::from_secs(300);

use super::witness::ToolFailure;

/// Runs `program` with `args` in `dir` (a folder of the repository); its output, or what it said
/// when it failed.
pub(crate) fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    run_witnessed(dir, program, args).map_err(ToolFailure::into_raw)
}

pub(super) fn run_witnessed(
    dir: &Path,
    program: &str,
    args: &[&str],
) -> Result<String, ToolFailure> {
    if let Some(target) = crate::remote::parse(dir) {
        let words: Vec<String> = std::iter::once(program)
            .chain(args.iter().copied())
            .map(crate::remote::sh_quote)
            .collect();
        let out = crate::remote::exec(
            &target.host,
            &crate::remote::Request {
                cwd: Some(target.path.clone()),
                script: format!("exec {}", words.join(" ")),
                long: true,
                timeout: Some(TIME),
                ..crate::remote::Request::default()
            },
        )
        .map_err(ToolFailure::Opaque)?;
        return finish_witnessed(
            program,
            out.code == 0,
            &out.stdout,
            &out.stderr,
            out.code == 127,
        );
    }
    // npm is a batch file on Windows.
    let exe = if cfg!(windows) && program == "npm" {
        "npm.cmd"
    } else {
        program
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        // Plain output, as it is read.
        .env("NO_COLOR", "1")
        .env("CARGO_TERM_COLOR", "never");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(missing_witnessed(program));
        }
        Err(e) => return Err(run_failure(program, e.to_string())),
    };
    // Read as it comes, so that a full pipe never holds the tool up.
    fn drain<R: std::io::Read + Send + 'static>(
        pipe: Option<R>,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let until = std::time::Instant::now() + TIME;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(timeout_failure(program));
            }
            Err(e) => return Err(run_failure(program, e.to_string())),
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    finish_witnessed(program, status.success(), &out, &err, false)
}

fn missing(program: &str) -> String {
    missing_witnessed(program).into_raw()
}

fn missing_witnessed(program: &str) -> ToolFailure {
    ToolFailure::Missing {
        raw: format!(
            "{program} wasn't found where the repository is (not installed, or not on its PATH): \
             it is what lists the package's files"
        ),
        program: program.to_owned(),
    }
}

fn run_failure(program: &str, detail: String) -> ToolFailure {
    ToolFailure::Run {
        raw: format!("Couldn't run {program}: {detail}"),
        program: program.to_owned(),
        detail,
    }
}

fn timeout_failure(program: &str) -> ToolFailure {
    let minutes = TIME.as_secs() / 60;
    ToolFailure::Timeout {
        raw: format!(
            "{program} took longer than {minutes} minutes (waiting for a lock, or the network?): stopped"
        ),
        program: program.to_owned(),
        minutes,
    }
}

fn finish(
    program: &str,
    ok: bool,
    stdout: &[u8],
    stderr: &[u8],
    not_found: bool,
) -> Result<String, String> {
    finish_witnessed(program, ok, stdout, stderr, not_found).map_err(ToolFailure::into_raw)
}

pub(super) fn finish_witnessed(
    program: &str,
    ok: bool,
    stdout: &[u8],
    stderr: &[u8],
    not_found: bool,
) -> Result<String, ToolFailure> {
    if ok {
        return Ok(String::from_utf8_lossy(stdout).into_owned());
    }
    if not_found {
        return Err(missing_witnessed(program));
    }
    let said = String::from_utf8_lossy(stderr).trim().to_owned();
    Err(if said.is_empty() {
        ToolFailure::Failed {
            raw: format!("{program} failed"),
            program: program.to_owned(),
        }
    } else {
        ToolFailure::Opaque(said)
    })
}

#[cfg(test)]
#[path = "run/display_tests.rs"]
mod display_tests;

#[cfg(test)]
#[path = "run/witness_tests.rs"]
mod witness_tests;
