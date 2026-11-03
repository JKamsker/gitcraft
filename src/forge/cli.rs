//! Running a forge's own command-line tool (`gh`, `glab`), which holds the user's login.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Runs `tool` in `repo`; `noun` says what it's for when it isn't installed.
pub(super) fn run(tool: &str, noun: &str, repo: &Path, args: &[&str]) -> Result<String, String> {
    run_with_input(tool, noun, repo, args, None)
}

/// [`run`], with `input` on its stdin.
pub(super) fn run_with_input(
    tool: &str,
    noun: &str,
    repo: &Path,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, String> {
    let mut cmd = Command::new(tool);
    cmd.current_dir(repo).args(args);
    cmd.env("GH_PROMPT_DISABLED", "1").env("NO_PROMPT", "1");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let not_installed = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound if repo.is_dir() => {
            format!("{tool} isn't installed: gitcraft uses it for {noun}s")
        }
        _ => format!("couldn't run {tool} in {}: {e}", repo.display()),
    };
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
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
