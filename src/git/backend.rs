//! Capability-based local reads. Unsupported contexts retain the installed Git unchanged.
//!
//! Native work is synchronous filesystem/CPU work and stays on the existing bounded read
//! workers. It does not run on the UI thread or claim to provide asynchronous network I/O.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub(super) enum Read {
    Refs,
    Head,
    Remotes,
    GitDir,
    CommitFiles,
    CommitDetail,
    DiffContext,
    FileDiff,
    FileAt,
    CommitFileSides,
    Textconv,
    Authorship,
}
impl Read {
    fn name(self) -> &'static str {
        match self {
            Self::Refs => "refs",
            Self::Head => "head",
            Self::Remotes => "remotes",
            Self::GitDir => "git-dir",
            Self::CommitFiles => "commit-files",
            Self::CommitDetail => "commit-detail",
            Self::DiffContext => "diff-context",
            Self::FileDiff => "file-diff",
            Self::FileAt => "file-at",
            Self::CommitFileSides => "commit-file-sides",
            Self::Textconv => "textconv",
            Self::Authorship => "authorship",
        }
    }
}

/// Cumulative, bounded samples for a read or CLI subcommand. No repository names, arguments,
/// URLs or credentials are retained. Samples are the latest 1024 calls, in microseconds.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct OperationMetrics {
    pub operation: String,
    pub native_calls: u64,
    pub fallback_calls: u64,
    pub subprocesses: u64,
    pub elapsed_us: Vec<u64>,
    pub fallback_reasons: BTreeMap<String, u64>,
}
static METRICS: OnceLock<Mutex<BTreeMap<String, OperationMetrics>>> = OnceLock::new();
fn counters() -> &'static Mutex<BTreeMap<String, OperationMetrics>> {
    METRICS.get_or_init(Default::default)
}
/// A snapshot suitable for diagnostics and benchmark deltas.
pub fn metrics() -> Vec<OperationMetrics> {
    counters()
        .lock()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default()
}
/// Reset benchmark samples. Call only while no repository operations are running.
#[cfg(test)]
pub(crate) fn reset_metrics() {
    if let Ok(mut m) = counters().lock() {
        m.clear();
    }
}
fn record(name: &str, native: bool, process: bool, reason: Option<&str>, start: Instant) {
    if native || process {
        crate::timing::git(name, native, start.elapsed());
    }
    if let Ok(mut all) = counters().lock() {
        let m = all
            .entry(name.to_owned())
            .or_insert_with(|| OperationMetrics {
                operation: name.to_owned(),
                ..Default::default()
            });
        m.native_calls += u64::from(native);
        m.fallback_calls += u64::from(!native && !process);
        m.subprocesses += u64::from(process);
        if let Some(reason) = reason {
            *m.fallback_reasons.entry(reason.to_owned()).or_default() += 1;
        }
        if m.elapsed_us.len() == 1024 {
            m.elapsed_us.remove(0);
        }
        m.elapsed_us
            .push(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
    }
}
/// Records dispatched Git calls, including unsupported operations and remote invocations.
pub(super) struct CliTimer {
    name: String,
    start: Instant,
    /// The whole command, when the timing log writes every call's (see [`crate::timing::all`]).
    args: Option<String>,
}
impl CliTimer {
    pub(super) fn new(args: &[&str]) -> Self {
        let mut rest = args;
        while let Some(word) = rest.first() {
            if matches!(*word, "-c" | "-C") {
                rest = rest.get(2..).unwrap_or_default();
            } else if word.starts_with('-') {
                rest = &rest[1..];
            } else {
                break;
            }
        }
        crate::timing::git_begins();
        Self {
            name: format!("cli:{}", rest.first().unwrap_or(&"unknown")),
            start: Instant::now(),
            args: crate::timing::all().then(|| {
                args.iter()
                    .map(|a| super::redact(a, None))
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        }
    }
}
impl Drop for CliTimer {
    fn drop(&mut self) {
        record(&self.name, false, true, None, self.start);
        if let Some(args) = &self.args {
            crate::timing::log(format_args!("  git {args}"));
        }
    }
}

/// Gitoxide discovers installation configuration through a bare `git` command, using
/// the process PATH. GitCraft itself uses the sanitized/login-shell path. Never initialize
/// the native library when those could select different programs or search a relative path.
#[cfg(unix)]
fn git_executable_context_matches(
    selected: Option<&Path>,
    inherited: &std::ffi::OsStr,
) -> Option<PathBuf> {
    // Resolution stops at the first executable Git. A later relative entry cannot
    // affect that result, but one before it could execute code from the working tree.
    let mut inherited_git = None;
    for dir in std::env::split_paths(inherited) {
        if !dir.is_absolute() {
            return None;
        }
        let file = dir.join("git");
        if file.is_file() && rustix::fs::access(&file, rustix::fs::Access::EXEC_OK).is_ok() {
            inherited_git = Some(file);
            break;
        }
    }
    let inherited_git = inherited_git?;
    let selected = std::fs::canonicalize(selected?).ok()?;
    let inherited_git = std::fs::canonicalize(inherited_git).ok()?;
    (selected == inherited_git).then_some(selected)
}

fn capability(repo: &Path) -> Result<(), &'static str> {
    if !cfg!(feature = "native-git") {
        return Err("feature-disabled");
    }
    if crate::practice::is_repository(repo) {
        return Err("practice-environment");
    }
    if std::env::var_os("GITCRAFT_GIT_BACKEND").is_some_and(|v| v == "cli") {
        return Err("explicit-cli");
    }
    if crate::remote::is_remote(repo) {
        return Err("remote-context");
    }
    if crate::connections::current().error.is_some() {
        return Err("invalid-profile");
    }
    if super::profile_config().is_some() {
        return Err("profile-config");
    }
    // The installed Git remains authoritative for inherited repository/config overrides.
    // In particular never accidentally read local objects for a GIT_DIR/GIT_WORK_TREE setup.
    if std::env::vars_os().any(|(k, _)| {
        k.to_str().is_some_and(|k| {
            k.starts_with("GIT_")
                && !matches!(
                    k,
                    "GIT_TERMINAL_PROMPT"
                        | "GIT_OPTIONAL_LOCKS"
                        | "GIT_EDITOR"
                        | "GIT_SEQUENCE_EDITOR"
                        | "GIT_PAGER"
                        // Credentials and transport only, never a local read. Editors' terminals
                        // (VS Code's) set GIT_ASKPASS for everything started from them.
                        | "GIT_ASKPASS"
                        | "GIT_SSH"
                        | "GIT_SSH_COMMAND"
                        | "GIT_SSH_VARIANT"
                )
        })
    }) {
        return Err("git-environment");
    }
    #[cfg(windows)]
    if std::env::var_os("EXEPATH").is_some() || crate::shell_path::path().is_some() {
        // Gitoxide can derive installation config directly from EXEPATH, bypassing
        // the Git executable the app would run. Its helper also inherits the unsanitized
        // PATH. Retain Git whenever either context could differ on Windows.
        return Err("git-executable-context");
    }
    #[cfg(unix)]
    {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let selected = crate::shell_path::executable("git");
        let executable = git_executable_context_matches(selected.as_deref(), &inherited)
            .ok_or("git-executable-context")?;
        // Gitoxide caches installation discovery process-wide. If a different executable
        // later wins resolution, keep Git authoritative instead of reusing that old config.
        static NATIVE_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
        if NATIVE_EXECUTABLE.get_or_init(|| executable.clone()) != &executable {
            return Err("git-executable-context");
        }
    }
    Ok(())
}

pub(super) fn dispatch<T>(
    repo: &Path,
    read: Read,
    native: impl FnOnce(&Path) -> Result<T, String>,
    cli: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let start = Instant::now();
    let reason = match capability(repo) {
        Ok(()) => match native(repo) {
            Ok(value) => {
                record(read.name(), true, false, None, start);
                return Ok(value);
            }
            Err(error) => match error.as_str() {
                "history-overlays" => "history-overlays",
                "legacy or rewritten remote" => "remote-url-config",
                "remote has no fetch URL" => "remote-without-fetch-url",
                "remote display requires Git" => "remote-display-format",
                "tracking graph exceeds native bound" => "tracking-graph-limit",
                "ambiguous upstream short name" => "ambiguous-upstream",
                "symbolic reference requires Git" => "symbolic-reference",
                "non-commit branch requires Git" => "non-commit-branch",
                "unsupported upstream namespace" => "upstream-namespace",
                "local upstream requires Git" => "local-upstream",
                "listed by git" => "listed-by-git",
                "commit encoding" => "commit-encoding",
                "diff options" => "diff-options",
                "diff driver" => "diff-driver",
                "not a plain file" => "not-a-plain-file",
                "not in the commit" => "not-in-the-commit",
                _ => "native-read-error",
            },
        },
        Err(reason) => reason,
    };
    let result = cli();
    record(read.name(), false, false, Some(reason), start);
    result
}

pub(super) fn head(repo: &Path) -> Result<String, String> {
    native::head(repo)
}
pub(super) fn refs(repo: &Path) -> Result<super::Refs, String> {
    native::refs(repo)
}
pub(super) fn remotes(repo: &Path) -> Result<Vec<super::Remote>, String> {
    native::remotes(repo)
}
pub(super) fn git_dir(repo: &Path) -> Result<PathBuf, String> {
    native::git_dir(repo)
}
/// Each commit's hash and files; none for those git is to list.
pub(super) type ListedFiles = Vec<(String, Option<Vec<super::ChangedFile>>)>;

/// The files of each of `hashes`, read in-process; none for those git is to list.
pub(super) fn commit_files(repo: &Path, hashes: &[String]) -> Result<ListedFiles, String> {
    #[cfg(feature = "native-git")]
    return commits::files(repo, hashes);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, hashes);
        Err("feature disabled".into())
    }
}
pub(super) fn commit_detail(repo: &Path, hash: &str) -> Result<super::CommitDetail, String> {
    #[cfg(feature = "native-git")]
    return commits::detail(repo, hash);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, hash);
        Err("feature disabled".into())
    }
}

pub(super) fn diff_context(repo: &Path) -> Result<Option<i64>, String> {
    #[cfg(feature = "native-git")]
    return commits::diff_context(repo);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = repo;
        Err("feature disabled".into())
    }
}

pub(super) fn file_patch(
    repo: &Path,
    hash: &str,
    file: &str,
    opts: super::DiffOptions,
) -> Result<String, String> {
    #[cfg(feature = "native-git")]
    return patch::file_patch(repo, hash, file, opts);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, hash, file, opts);
        Err("feature disabled".into())
    }
}

pub(super) fn file_at(repo: &Path, rev: &str, file: &str) -> Result<Vec<u8>, String> {
    #[cfg(feature = "native-git")]
    return patch::file_at(repo, rev, file);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, rev, file);
        Err("feature disabled".into())
    }
}

pub(super) fn commit_file_sides(
    repo: &Path,
    hash: &str,
    file: &str,
) -> Result<super::FileSides, String> {
    #[cfg(feature = "native-git")]
    return patch::commit_file_sides(repo, hash, file);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, hash, file);
        Err("feature disabled".into())
    }
}

pub(super) fn textconv_of(repo: &Path, file: &str) -> Result<Option<String>, String> {
    #[cfg(feature = "native-git")]
    return patch::textconv_of(repo, file);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, file);
        Err("feature disabled".into())
    }
}

pub(super) fn authorship(
    repo: &Path,
    hash: &str,
    notes: &[&str],
) -> Result<super::Authorship, String> {
    #[cfg(feature = "native-git")]
    return patch::authorship(repo, hash, notes);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, hash, notes);
        Err("feature disabled".into())
    }
}

/// Git's exit code, output and error output for a read answered from the repository's files
/// in-process (see `answer`); `None` when git is to run.
pub(super) fn answer(repo: &Path, args: &[&str]) -> Option<(i32, Vec<u8>, Vec<u8>)> {
    #[cfg(feature = "native-git")]
    {
        let start = Instant::now();
        capability(repo).ok()?;
        let answer = answer::answer(repo, args)?;
        record(answer::name(args), true, false, None, start);
        Some((answer.code, answer.stdout, answer.stderr))
    }
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, args);
        None
    }
}

#[cfg(feature = "native-git")]
mod answer;

/// Read an unfiltered blob up to `limit` bytes, falling back when native Git is unsuitable.
pub(super) fn bounded_blob(repo: &Path, spec: &str, limit: usize) -> Option<Result<Vec<u8>, ()>> {
    #[cfg(feature = "native-git")]
    {
        let start = Instant::now();
        capability(repo).ok()?;
        let result = answer::bounded_blob(repo, spec, limit)?;
        // Timing only: never record a private comparison's spec or content.
        record("raw-blob", true, false, None, start);
        Some(result)
    }
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (repo, spec, limit);
        None
    }
}
#[cfg(feature = "native-git")]
mod history;

/// Cache eligible history logs when their Git inputs have not changed.
pub(super) fn history_log(
    path: &Path,
    args: &[String],
    eligible: bool,
    read: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    #[cfg(feature = "native-git")]
    return history::read(path, args, eligible, read);
    #[cfg(not(feature = "native-git"))]
    {
        let _ = (path, args, eligible);
        read()
    }
}
#[cfg(feature = "native-git")]
mod commits;
#[cfg(feature = "native-git")]
mod patch;

#[cfg(not(feature = "native-git"))]
#[path = "backend/native_disabled.rs"]
mod native;

#[cfg(feature = "native-git")]
#[path = "backend/native_enabled.rs"]
mod native;

#[cfg(all(test, feature = "native-git"))]
mod tests;

#[cfg(all(test, unix))]
mod executable_context_tests;
