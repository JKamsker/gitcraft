//! Quoted shell command envelopes for local and remote repositories.

use super::*;

/// The alias a shell script is run through (see [`run_shell`]).
pub(super) const SHELL_ALIAS: &str = "gitcraft-run";

/// Defines [`SHELL_ALIAS`]: git runs a `!` alias with its own `sh` (Git for Windows' on
/// Windows), in the top folder of the working tree, with the script as its one argument.
pub(super) const SHELL_ALIAS_DEF: &str = "alias.gitcraft-run=!f() { eval \"$1\"; }; f";

/// [`SHELL_ALIAS_DEF`] for a script handed over in [`SHELL_SCRIPT_VAR`]: in the environment,
/// not on a command line, where anyone on that machine could read it (`ps`) with the answers
/// it holds.
pub(super) const SHELL_ALIAS_ENV_DEF: &str = "alias.gitcraft-run=!eval \"$GITCRAFT_SCRIPT\"";

/// The variable [`run_shell`] hands its script over in.
pub(super) const SHELL_SCRIPT_VAR: &str = "GITCRAFT_SCRIPT";

/// How a shell script ended, and what it printed.
pub struct ShellRun {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs `script` with `sh` in the repository `dir`, on its machine (WSL, SSH: there), through
/// git, which has a `sh` wherever it is installed. What it needs to know goes in the script as
/// quoted variables, never into its text. A failing script is an `Ok` with its exit code;
/// `Err` only when it couldn't be run.
pub fn run_shell(dir: &Path, script: &str) -> Result<ShellRun, String> {
    let extra = Extra {
        pre: vec!["-c", SHELL_ALIAS_ENV_DEF],
        env: vec![(SHELL_SCRIPT_VAR, script.to_owned())],
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, &[SHELL_ALIAS])?;
    Ok(ShellRun {
        // Ended by a signal: no code, but not a success.
        code: out.code.unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// The program and arguments that run `script` as [`run_shell`] does, in the repository `dir`
/// of this machine, for a terminal to run it in (where it can be watched and answered).
pub fn shell_command_here(dir: &Path, script: &str) -> (String, Vec<String>) {
    let args = [
        "-C".to_owned(),
        dir.display().to_string(),
        "-c".to_owned(),
        SHELL_ALIAS_DEF.to_owned(),
        SHELL_ALIAS.to_owned(),
        script.to_owned(),
    ];
    ("git".to_owned(), args.to_vec())
}
