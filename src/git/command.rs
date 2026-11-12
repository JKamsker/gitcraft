//! Running git: the environment every call gets, credentials for remotes, errors. A call for a
//! repository in WSL or over SSH runs git there (see [`crate::remote`]), with the same
//! environment.

pub(super) mod bounded;
mod memo;
mod source;
pub use source::CommandSource;

use std::path::{Path, PathBuf};
use std::process::Command;
/// The initiating task's immutable profile config (including queued and nested reads).
pub fn profile_config() -> Option<PathBuf> {
    crate::connections::current().git_config
}

/// Whether `args` reads or writes the user's own global config: `git config --global …`
/// (signing settings, Mergiraf's set-up) or `git maintenance` (its list of repositories), which
/// must never land in the profile's file, which is written over.
fn user_global_config(args: &[&str]) -> bool {
    let rest = after_options(args);
    (rest.first() == Some(&"config") && rest.contains(&"--global"))
        // `maintenance start` / `register` write `maintenance.repo` to the global config.
        || (rest.first() == Some(&"maintenance")
            && matches!(rest.get(1), Some(&("start" | "register" | "unregister" | "stop"))))
}

/// `args` from the subcommand on: past the options before it (`-c x=y`, `-C dir`,
/// `--literal-pathspecs`…).
pub(super) fn after_options<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
    super::parse_command_envelope(args)
        .map(|(at, _)| &args[at..])
        .unwrap_or(&[])
}

/// Points `cmd` at the active profile's config, unless `args` is about the user's own.
pub(super) fn with_profile(cmd: &mut Command, args: &[&str]) {
    if user_global_config(args) {
        return;
    }
    let context = crate::connections::current();
    if context.error.is_some() {
        cmd.arg("-c").arg("=invalid-profile");
        return;
    }
    if !context.choices.ssh_key.is_empty() {
        cmd.env_remove("GIT_SSH_COMMAND")
            .env_remove("GIT_SSH")
            .env_remove("GIT_SSH_VARIANT");
    }
    let path = context.git_config;
    if let Some(path) = path {
        cmd.env("GIT_CONFIG_GLOBAL", path);
    }
}

use crate::remote::{self, Target};

mod connection;
mod credential_policy;
mod endpoints;
#[cfg(all(test, unix))]
mod native_https_fixture;
pub(crate) use credential_policy::*;
pub(crate) use endpoints::*;
mod setup;
pub(crate) use connection::*;
pub(crate) use setup::{setup_clone, setup_fetch, setup_path, setup_run};
mod watched;
pub use watched::{CANCELLED, OpControl, run_watched, run_watched_recorded};

/// What every git call is run with.
/// - `GIT_TERMINAL_PROMPT`: never block on an interactive prompt (credential helpers with a
///   GUI still work).
/// - `GIT_EDITOR`: `rebase --continue`, `merge --continue`… would otherwise open an editor
///   nobody sees.
/// - `LANGUAGE`: messages in English whatever the user's language: errors are told apart by
///   their words (`is_auth_error`…). `C` in LANGUAGE wins over LC_ALL / LANG for messages only,
///   so the character set (and what hooks see of it) stays the user's.
pub(super) const GIT_ENV: [(&str, &str); 4] = [
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_EDITOR", "true"),
    ("LANGUAGE", "C"),
];

/// Interactive rebases set `sequence.editor` themselves; an inherited one would win.
pub(super) const GIT_ENV_REMOVED: [&str; 1] = ["GIT_SEQUENCE_EDITOR"];

/// A tab's explicit repository path selects its repository for both guards and
/// mutations. Inherited shell plumbing must not silently select another index,
/// namespace, object store or shallow boundary. Deliberate per-command overrides
/// are installed afterwards through `Extra::env` (private indexes, for example).
pub(super) const REPOSITORY_ENV_REMOVED: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_SHALLOW_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_GRAFT_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_ATTR_SOURCE",
];

/// Pure environment portion of the shared selected-repository command builder.
fn clear_repository_environment(cmd: &mut Command) {
    for name in REPOSITORY_ENV_REMOVED {
        cmd.env_remove(name);
    }
}

pub(super) fn git_cmd() -> Command {
    // With a bare name and an overridden PATH, Rust falls back to fork/exec on Unix.
    // Resolve through the child's PATH first, allowing its much cheaper posix_spawn path.
    // Look up each time so installing or removing a git still takes effect while running.
    #[cfg(unix)]
    let mut cmd =
        Command::new(crate::shell_path::executable("git").unwrap_or_else(|| "git".into()));
    #[cfg(not(unix))]
    let mut cmd = Command::new("git");
    for (name, value) in GIT_ENV {
        cmd.env(name, value);
    }
    if let Some((name, value)) = offline_env() {
        cmd.env(name, value);
    }
    for name in GIT_ENV_REMOVED {
        cmd.env_remove(name);
    }
    clear_repository_environment(&mut cmd);
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// How a git call ended, here or elsewhere.
struct Ran {
    code: Option<i32>,
    /// The exit status as it reads in an error.
    status: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// The hook that failed, when the call's hooks were traced.
    failed_hook: Option<super::hooks::FailedHook>,
}

/// Options before the subcommand, and variables, a call is run with on top of [`GIT_ENV`].
#[derive(Clone, Default)]
pub(super) struct Extra {
    /// Normalize before installing intentional command-owned environment.
    normalize_repository: bool,
    unset: Vec<&'static str>,
    output_limit: Option<usize>,
    stdout_file: Option<PathBuf>,
    pre: Vec<&'static str>,
    owned_pre: Vec<String>,
    env: Vec<(&'static str, String)>,
    /// What git reads on its input; nothing when `None`.
    stdin: Option<Vec<u8>>,
    /// It may take minutes (a large bundle): on another machine, run where that is allowed.
    long: bool,
    /// It starts a program the user works in (a diff or merge tool) and waits until it closes.
    tool: bool,
    /// Find out which hook failed, if one does (see [`super::hooks`]).
    trace_hooks: bool,
    /// On another machine: a test of a parallel bisect (see [`run_for_test_input`]).
    test: Option<std::time::Duration>,
    /// How long it may run on another machine before it is stopped there.
    timeout: Option<std::time::Duration>,
}

impl Extra {
    /// Selected-repository inspection/mutation; owned private overrides are added
    /// after normalization by the shared production command builders.
    pub(in crate::git) fn repository() -> Self {
        Self {
            normalize_repository: true,
            ..Self::default()
        }
    }
}

pub(in crate::git) use watched::run_watched_extra_recorded;

/// How long a diff or merge tool started on another machine may stay open before it is stopped.
const TOOL_FOR: std::time::Duration = std::time::Duration::from_secs(12 * 3600);

thread_local! {
    /// Git runs on this thread are reads that mustn't fetch (see [`offline`]).
    static OFFLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `work` with every git it starts on this thread kept from fetching what a partial
/// clone lacks (`GIT_NO_LAZY_FETCH`): for views that only read, a missing object is an error
/// rather than a download written into the repository.
pub fn offline<T>(work: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            OFFLINE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(OFFLINE.with(|o| o.replace(true)));
    work()
}

/// What every git started on this thread gets in its environment within [`offline`].
fn offline_env() -> Option<(&'static str, &'static str)> {
    OFFLINE
        .with(std::cell::Cell::get)
        .then_some(("GIT_NO_LAZY_FETCH", "1"))
}

impl Extra {
    /// Nothing on top of the call: no input, options, variables or output file. Only such a
    /// read is answered without git (see [`answered`]).
    fn plain(&self) -> bool {
        self.stdin.is_none()
            && self.pre.is_empty()
            && self.owned_pre.is_empty()
            && self.env.is_empty()
            && self.stdout_file.is_none()
            && !self.long
            && !self.tool
            && !self.trace_hooks
            && self.test.is_none()
            && self.timeout.is_none()
    }
}

/// How an exit status reads, as `ExitStatus` writes it.
fn exit_text(code: i32) -> String {
    if cfg!(windows) {
        format!("exit code: {code}")
    } else {
        format!("exit status: {code}")
    }
}

/// The git alias a command of the user's (a conflict decision's test) is run as, set with
/// `-c alias.<it>=!<command>`: git runs it with `sh` in the repository's top folder, wherever
/// the repository is.
pub const USER_COMMAND_ALIAS: &str = "gitcraft-user-command";

/// Whether the git command `args` (its options first) talks to a remote and may take minutes
/// (or is a command of the user's, which may too).
pub fn goes_over_network(args: &[&str]) -> bool {
    matches!(
        after_options(args).first().copied(),
        Some(
            "fetch"
                | "pull"
                | "push"
                | "clone"
                | "ls-remote"
                | "remote"
                | "submodule"
                | "lfs"
                | SHELL_ALIAS
                | USER_COMMAND_ALIAS
                | super::dvc::PULL_ALIAS
        )
    )
}

/// Runs git in `dir` and returns stdout, or the error output.
pub fn run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    run_ok_codes(dir, args, &[0])
}

/// [`run`], with `input` on git's standard input (`notes add -F -`: a text of any size, which
/// a command line would limit).
pub fn run_input(dir: Option<&Path>, args: &[&str], input: &str) -> Result<String, String> {
    let extra = Extra {
        stdin: Some(input.as_bytes().to_vec()),
        ..Extra::default()
    };
    finish(execute(dir, &extra, args)?, args, &[0])
}

/// [`run`] for a read nobody asked for and nobody waits on (the start page's look at a recent
/// repository): for one on another machine, only in a shell already open there, so it never
/// connects (no password asked, no WSL started); `None` when there is none.
pub fn run_if_connected(dir: &Path, args: &[&str]) -> Option<Result<String, String>> {
    let Some(target) = remote::parse(dir) else {
        return Some(run(Some(dir), args));
    };
    let _timer = super::backend::CliTimer::new(args);
    let mut req = remote_request(&target, &Extra::default(), args);
    req.timeout = Some(std::time::Duration::from_secs(30));
    let out = match remote::exec_if_connected(&target.host, &req)? {
        Ok(out) => out,
        Err(e) => return Some(Err(e)),
    };
    Some(if out.code == 0 {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else if out.code == 1 && remote::is_missing_folder(&out.stderr) {
        Err(missing_folder(&target))
    } else {
        Err(download_read_error(
            args,
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    })
}

#[cfg(test)]
mod r2_inert_tests;

/// Like [`run`], but exit codes in `ok` count as success (`diff --no-index` exits 1 on changes).
pub fn run_ok_codes(dir: Option<&Path>, args: &[&str], ok: &[i32]) -> Result<String, String> {
    run_inner(dir, args, ok, None, false)
}

/// [`run_with`] for an operation someone asked for: when one of its hooks makes it fail (a
/// commit's `pre-commit`, a push's `pre-push`…), the error names that hook first.
pub fn run_naming_hooks(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
) -> Result<String, String> {
    run_extra(
        dir,
        args,
        &[0],
        creds,
        Extra {
            trace_hooks: super::hooks::wants_trace(args),
            ..Extra::default()
        },
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reviewed_push_tests;

#[cfg(test)]
mod practice_environment_tests;

#[cfg(test)]
mod download_policy_recording_tests;

#[cfg(all(test, unix))]
mod sparse_account_recording_tests;

#[cfg(test)]
mod repository_endpoint_recording;

#[cfg(test)]
mod internals_merge_recording;

#[cfg(test)]
mod backend_fluent_identity_tests;

mod repository;
pub(crate) use repository::local_repair_input;
pub(crate) use repository::onboarding_local_config;
use repository::practice_command;
pub(crate) use repository::practice_run;
pub(crate) use repository::repair_host;
pub(super) use repository::repository_command;
pub(in crate::git) use repository::repository_endpoint;
#[cfg(test)]
use repository::repository_endpoint_extra;
pub(crate) use repository::tour_read;

mod reviewed_push;
use reviewed_push::PREFLIGHT_REMOVE;
pub(crate) use reviewed_push::PushTarget;
pub(crate) use reviewed_push::push_preflight_with;
#[cfg(test)]
use reviewed_push::reviewed_push_extra;
#[cfg(test)]
use reviewed_push::reviewed_refusal;

mod object_reads;
pub(super) use object_reads::comparison_output;
pub(super) use object_reads::comparison_remote;
use object_reads::download_read_error;
pub(crate) use object_reads::draft_bytes;
pub(crate) use object_reads::forecast_bytes;
pub(crate) use object_reads::forecast_remote;
pub(crate) use object_reads::forecast_run;
use object_reads::object_read_policy;
pub(super) use object_reads::recovery_output;
pub(crate) use object_reads::syntax_read;
#[cfg(test)]
use object_reads::syntax_read_error;

mod result;
pub use result::Recorded;
use result::both;
use result::extra_for;
use result::failure_message;
use result::finish;
pub use result::run_env_owned;
use result::run_extra;
use result::run_inner;
use result::run_inner_extra;
pub use result::run_recorded;
pub use result::run_with_index;

mod workspace;
pub use workspace::Call;
pub use workspace::Exit;
pub use workspace::ToolRun;
pub(crate) use workspace::WorkspaceCommand;
pub use workspace::run_call;
pub(super) use workspace::run_coded;
pub(crate) use workspace::run_for_test_input;
pub(crate) use workspace::run_owned_test;
pub use workspace::run_tool;
pub(crate) use workspace::run_workspace_command;

mod output;
pub use output::Captured;
#[cfg(test)]
use output::inert_object_options;
pub use output::run_bytes;
pub use output::run_captured;
pub use output::run_code;
pub use output::run_env;
pub use output::run_env_codes;
pub use output::run_inert_objects;
pub use output::run_to_file;
pub(crate) use output::transfer_pack;

mod shell;
use shell::SHELL_ALIAS;
use shell::SHELL_ALIAS_ENV_DEF;
use shell::SHELL_SCRIPT_VAR;
pub use shell::ShellRun;
pub use shell::run_shell;
pub use shell::shell_command_here;

mod remote_execution;
#[allow(unused_imports)] // Compatibility re-export.
pub(super) use remote_execution::CONNECTION_REDIRECT_ENV;
use remote_execution::execute_there;
pub(super) use remote_execution::missing_folder;
use remote_execution::normalize_connection_command;
pub(super) use remote_execution::remote_request;

mod local_execution;
#[allow(unused_imports)] // Compatibility re-export.
pub(super) use local_execution::apply_command_extras;
use local_execution::execute;
pub(super) use local_execution::local_command;

mod credentials;
pub use credentials::Credentials;
pub use credentials::is_auth_error;
#[cfg(test)]
use credentials::parse_credentials;
pub use credentials::run_unattended;
pub use credentials::run_unattended_within;
pub use credentials::run_with;
pub use credentials::stored_credentials;
#[cfg(all(test, unix))]
use credentials::unattended;
use credentials::unattended_extra;

#[cfg(test)]
mod global_option_tests;
