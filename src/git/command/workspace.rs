//! Workspace and tool command entry points with explicit exit-code handling.

use super::*;

/// How a diff or merge tool started through git ended.
#[derive(Debug)]
pub struct ToolRun {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `git difftool` / `git mergetool` (`args`) in `dir`, on whichever machine it is, and
/// waits for the tool it starts to close, however long it stays open (on another machine, at
/// most [`TOOL_FOR`]). Whatever git exits with is returned, not taken for an error: the caller
/// tells a cancelled merge from a failed one.
pub fn run_tool(dir: &Path, args: &[&str]) -> Result<ToolRun, String> {
    let extra = Extra {
        tool: true,
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    Ok(ToolRun {
        code: out.code,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Runs git in `dir` (on this machine) and returns its exit code and output: stdout when it
/// succeeds, stderr when it doesn't. For a caller that reads failures as answers (see
/// [`super::overview`]), which runs a repository elsewhere through one script instead.
pub(in crate::git) fn run_coded(dir: &Path, args: &[&str]) -> Result<(i32, Vec<u8>), String> {
    let out = execute(Some(dir), &Extra::default(), args)?;
    match out.code {
        Some(0) => Ok((0, out.stdout)),
        code => Ok((code.unwrap_or(-1), out.stderr)),
    }
}

/// What a [`run_call`] is run with besides its arguments.
#[derive(Default)]
pub struct Call {
    /// Variables set for it (`GIT_INDEX_FILE`…).
    pub env: Vec<(&'static str, String)>,
    /// What it reads on its input.
    pub stdin: Option<Vec<u8>>,
    /// It may take minutes (see `remote::Request::long`).
    pub long: bool,
}

/// Runs git with `args` in `dir` as `call` says, and returns its output as raw bytes, or the
/// error output.
pub fn run_call(dir: Option<&Path>, args: &[&str], call: Call) -> Result<Vec<u8>, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: call.env,
        stdin: call.stdin,
        long: call.long,
        ..Extra::default()
    };
    let out = execute(dir, &extra, args)?;
    if out.code == Some(0) {
        return Ok(out.stdout);
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(if said.is_empty() {
        crate::i18n::message(
            "backend-git-failed-da095f",
            &[
                ("args", (args.join(" ")).to_string().into()),
                ("status", (out.status).to_string().into()),
            ],
        )
    } else {
        said
    })
}

/// How a git call ended: its exit code (`None`: ended by a signal) and what it printed.
#[derive(Clone, Debug)]
pub struct Exit {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) fn run_for_test_input(
    dir: &Path,
    args: &[&str],
    limit: std::time::Duration,
    stdin: Option<Vec<u8>>,
) -> Result<Exit, String> {
    let extra = Extra {
        test: Some(limit),
        stdin,
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    Ok(Exit {
        code: out.code,
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Immutable actual-machine authority for one durable workspace operation.
pub(crate) struct WorkspaceCommand<'a> {
    pub authority: &'a remote::workspace_authority::WorkspaceAuthority,
    pub input: Option<Vec<u8>>,
    pub cleanup: bool,
    pub directory: Option<&'a Path>,
}

pub(crate) fn run_owned_test(
    dir: &Path,
    args: &[&str],
    record: &Path,
    limit: std::time::Duration,
    cancel: crate::tasks::Cancellation,
    authority: &remote::workspace_authority::WorkspaceAuthority,
) -> Result<crate::processes::Output, String> {
    run_workspace_command(
        dir,
        args,
        record,
        limit,
        cancel,
        WorkspaceCommand::new(authority),
    )
}

pub(crate) fn run_workspace_command(
    dir: &Path,
    args: &[&str],
    record: &Path,
    limit: std::time::Duration,
    cancel: crate::tasks::Cancellation,
    owned: WorkspaceCommand<'_>,
) -> Result<crate::processes::Output, String> {
    let extra = Extra {
        test: Some(limit),
        stdin: owned.input.clone(),
        ..Default::default()
    };
    let mut options = crate::processes::Options {
        cancel,
        background: true,
        timeout: Some(limit),
        ..Default::default()
    };
    let command = if let Some(target) = remote::parse(dir) {
        let record = remote::parse(record).ok_or(crate::i18n::text(
            "backend-remote-owner-record-is-on-a-different-machine-5eab4e",
        ))?;
        if record.host != target.host {
            return Err(crate::i18n::text(
                "backend-remote-owner-record-is-on-a-different-machine-5eab4e",
            )
            .into());
        }
        if owned.directory.is_some() {
            return Err(crate::i18n::text(
                "backend-remote-private-directories-use-their-fenced-shell-operation-9bbfda",
            )
            .into());
        }
        let request = remote_request(&target, &extra, args);
        remote::streaming_command_authority(
            &target.host,
            &request,
            Some(&record.path),
            Some(owned.authority),
            owned.cleanup,
        )?
        .configure(&mut options)
    } else {
        if remote::parse(record).is_some() {
            return Err(crate::i18n::text(
                "backend-native-owner-record-is-on-a-different-machine-56f2b5",
            )
            .into());
        }
        remote::native_owner::command_with_authority(
            local_command(Some(dir), &extra, args),
            record,
            limit,
            Some(owned.authority.clone()),
        )?
        .with_input(owned.input.as_deref(), owned.cleanup)?
        .with_directory(owned.directory)?
        .configure(&mut options)
    };
    crate::processes::run(command, options)
}

impl<'a> WorkspaceCommand<'a> {
    pub fn new(authority: &'a remote::workspace_authority::WorkspaceAuthority) -> Self {
        Self {
            authority,
            input: None,
            cleanup: false,
            directory: None,
        }
    }
}
