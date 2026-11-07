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
}
impl Read {
    fn name(self) -> &'static str {
        match self {
            Self::Refs => "refs",
            Self::Head => "head",
            Self::Remotes => "remotes",
            Self::GitDir => "git-dir",
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
        Self {
            name: format!("cli:{}", rest.first().unwrap_or(&"unknown")),
            start: Instant::now(),
        }
    }
}
impl Drop for CliTimer {
    fn drop(&mut self) {
        record(&self.name, false, true, None, self.start);
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

#[cfg(not(feature = "native-git"))]
mod native {
    use super::*;
    pub(super) fn head(_: &Path) -> Result<String, String> {
        Err("feature disabled".into())
    }
    pub(super) fn refs(_: &Path) -> Result<super::super::Refs, String> {
        Err("feature disabled".into())
    }
    pub(super) fn remotes(_: &Path) -> Result<Vec<super::super::Remote>, String> {
        Err("feature disabled".into())
    }
    pub(super) fn git_dir(_: &Path) -> Result<PathBuf, String> {
        Err("feature disabled".into())
    }
}

#[cfg(feature = "native-git")]
mod native {
    use super::*;
    use gix::bstr::ByteSlice;
    use std::collections::HashSet;
    fn err(e: impl std::fmt::Display) -> String {
        e.to_string()
    }
    fn text(bytes: &gix::bstr::BStr) -> Result<String, String> {
        bytes.to_str().map(str::to_owned).map_err(err)
    }
    fn open(path: &Path) -> Result<gix::Repository, String> {
        let mut permissions = gix::open::Permissions::default();
        permissions.config.git_binary = true;
        let repo = gix::discover_opts(
            path,
            Default::default(),
            gix::open::Options::default()
                .permissions(permissions)
                .bail_if_untrusted(true)
                .strict_config(true),
        )
        .map_err(err)?;
        Ok(repo)
    }
    pub(super) fn head(path: &Path) -> Result<String, String> {
        let repo = open(path)?;
        let head = repo.head().map_err(err)?;
        if head.is_unborn() {
            return Ok(String::new());
        }
        Ok(head.id().map(|id| id.to_string()).unwrap_or_default())
    }
    pub(super) fn git_dir(path: &Path) -> Result<PathBuf, String> {
        let repo = open(path)?;
        std::fs::canonicalize(repo.git_dir()).map_err(err)
    }
    pub(super) fn remotes(path: &Path) -> Result<Vec<super::super::Remote>, String> {
        let repo = open(path)?;
        let config = repo.config_snapshot();
        // Git's URL rewriting and legacy remotes files remain the CLI's responsibility.
        if config.plumbing().sections_by_name("url").is_some()
            || repo.common_dir().join("remotes").exists()
            || repo.common_dir().join("branches").exists()
        {
            return Err("legacy or rewritten remote".into());
        }
        repo.remote_names()
            .into_iter()
            .map(|name| {
                let name = text(name.as_bstr())?;
                if name.contains(['\t', '\r', '\n']) {
                    return Err("remote display requires Git".into());
                }
                let values = config.plumbing().strings(&format!("remote.{name}.url"));
                let url = values
                    .and_then(|v| v.into_iter().next())
                    .ok_or("remote has no fetch URL")?;
                let url = text(url.as_bstr())?;
                if url.contains(['\r', '\n']) {
                    return Err("remote display requires Git".into());
                }
                Ok(super::super::Remote { name, url })
            })
            .collect()
    }
    fn reachable(
        repo: &gix::Repository,
        id: gix::ObjectId,
        remaining_visits: &mut usize,
    ) -> Result<HashSet<gix::ObjectId>, String> {
        let mut ids = HashSet::new();
        for info in repo
            .find_object(id)
            .map_err(err)?
            .id()
            .ancestors()
            .all()
            .map_err(err)?
        {
            if *remaining_visits == 0 {
                return Err("tracking graph exceeds native bound".into());
            }
            *remaining_visits -= 1;
            ids.insert(info.map_err(err)?.id);
        }
        Ok(ids)
    }
    pub(super) fn refs(path: &Path) -> Result<super::super::Refs, String> {
        refs_bounded(path, 100_000)
    }
    pub(super) fn refs_bounded(
        path: &Path,
        visit_budget: usize,
    ) -> Result<super::super::Refs, String> {
        let repo = open(path)?;
        if repo.common_dir().join("info/grafts").exists()
            || repo.common_dir().join("shallow").exists()
        {
            return Err("history-overlays".into());
        }
        let head = repo.head().map_err(err)?;
        let head_name = head
            .referent_name()
            .map(|name| text(name.as_bstr()))
            .transpose()?;
        let head_id = if head.is_unborn() {
            String::new()
        } else {
            head.id().map(|id| id.to_string()).unwrap_or_default()
        };
        let names: Vec<String> = repo
            .remote_names()
            .into_iter()
            .map(|n| text(n.as_bstr()))
            .collect::<Result<_, _>>()?;
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut out = super::super::Refs {
            head: head_id,
            ..Default::default()
        };
        let platform = repo.references().map_err(err)?;
        let mut list = platform
            .all()
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        if list
            .iter()
            .any(|r| r.name().as_bstr().starts_with(b"refs/replace/"))
        {
            return Err("history-overlays".into());
        }
        list.sort_by(|a, b| a.name().as_bstr().cmp(b.name().as_bstr()));
        // Bound total traversal work for the whole refresh, including repeated visits
        // to shared history across many diverged branches.
        let mut remaining_visits = visit_budget;
        for mut reference in list {
            let full = text(reference.name().as_bstr())?;
            let prefix = ["refs/heads/", "refs/remotes/", "refs/tags/"]
                .into_iter()
                .find(|p| full.starts_with(p));
            let Some(prefix) = prefix else {
                continue;
            };
            let short = &full[prefix.len()..];
            if prefix == "refs/remotes/" && short.ends_with("/HEAD") {
                continue;
            }
            let raw = reference
                .try_id()
                .ok_or("symbolic reference requires Git")?;
            let object = raw.object().map_err(err)?;
            let target = if object.kind == gix::object::Kind::Tag {
                if prefix != "refs/tags/" {
                    return Err("non-commit branch requires Git".into());
                }
                reference.peel_to_id().map_err(err)?.to_string()
            } else {
                raw.to_string()
            };
            let upstream = if prefix == "refs/heads/" {
                // Gitoxide's remote-tracking resolver returns None for the special
                // local repository remote. Git still displays and tracks that upstream.
                if repo
                    .config_snapshot()
                    .string(&format!("branch.{short}.remote"))
                    .is_some_and(|remote| remote.as_bstr() == b".")
                {
                    return Err("local upstream requires Git".into());
                }
                reference
                    .remote_tracking_ref_name(gix::remote::Direction::Fetch)
                    .transpose()
                    .map_err(err)?
            } else {
                None
            };
            let mut upstream_name = String::new();
            let mut track = String::new();
            if let Some(upstream) = upstream {
                let upstream_full = text(upstream.as_bstr())?;
                // Short-name ambiguity uses Git's exact disambiguation rules.
                let shortened = upstream_full
                    .strip_prefix("refs/remotes/")
                    .or_else(|| upstream_full.strip_prefix("refs/heads/"))
                    .ok_or("unsupported upstream namespace")?;
                // Git's DWIM expansion also considers refs/<name>, local branches,
                // remote refs and a remote's HEAD. Any competing spelling can change
                // %(upstream:short), so keep the CLI authoritative for that operation.
                let candidates = [
                    format!("refs/{shortened}"),
                    format!("refs/tags/{shortened}"),
                    format!("refs/heads/{shortened}"),
                    format!("refs/remotes/{shortened}"),
                    format!("refs/remotes/{shortened}/HEAD"),
                ];
                for candidate in candidates {
                    if candidate != upstream_full
                        && repo
                            .try_find_reference(candidate.as_str())
                            .map_err(err)?
                            .is_some()
                    {
                        return Err("ambiguous upstream short name".into());
                    }
                }
                // Literal pseudo-ref candidates live outside refs/. These filesystem
                // checks are conservative; an invalid candidate also safely uses Git.
                if repo.git_dir().join(shortened).is_file()
                    || repo.common_dir().join(shortened).is_file()
                {
                    return Err("ambiguous upstream short name".into());
                }
                upstream_name = shortened.to_owned();
                match repo
                    .try_find_reference(upstream_full.as_str())
                    .map_err(err)?
                {
                    None => track = "[gone]".into(),
                    Some(mut remote) => {
                        let local_id = reference.peel_to_id().map_err(err)?.detach();
                        let remote_id = remote.peel_to_id().map_err(err)?.detach();
                        if local_id != remote_id {
                            let ours = reachable(&repo, local_id, &mut remaining_visits)?;
                            let theirs = reachable(&repo, remote_id, &mut remaining_visits)?;
                            let ahead = ours.difference(&theirs).count();
                            let behind = theirs.difference(&ours).count();
                            track = match (ahead, behind) {
                                (0, 0) => String::new(),
                                (a, 0) => format!("[ahead {a}]"),
                                (0, b) => format!("[behind {b}]"),
                                (a, b) => format!("[ahead {a}, behind {b}]"),
                            };
                        }
                    }
                }
            }
            let branch = super::super::Branch {
                name: short.to_owned(),
                target,
                is_head: head_name.as_deref() == Some(full.as_str()),
                upstream: upstream_name,
                track,
            };
            match prefix {
                "refs/heads/" => {
                    if branch.is_head {
                        out.current = Some(branch.name.clone());
                    }
                    out.local.push(branch);
                }
                "refs/tags/" => out.tags.push(branch),
                _ => {
                    let remote = super::super::refs::remote_of(short, &names).to_owned();
                    match out.remotes.iter_mut().find(|(r, _)| r == &remote) {
                        Some((_, branches)) => branches.push(branch),
                        None => out.remotes.push((remote, vec![branch])),
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(all(test, feature = "native-git"))]
mod tests {
    use super::*;
    use crate::git::fixture::Repo;

    fn parity(repo: &Path) -> Result<(), String> {
        assert_eq!(native::refs(repo)?, super::super::refs::refs_cli(repo)?);
        assert_eq!(
            native::remotes(repo)?,
            super::super::refs::remotes_cli(repo)?
        );
        let expected = super::super::run(Some(repo), &["rev-parse", "--absolute-git-dir"])?;
        assert_eq!(
            native::git_dir(repo)?,
            std::fs::canonicalize(expected.trim()).map_err(|e| e.to_string())?
        );
        let expected = super::super::run(Some(repo), &["rev-parse", "--verify", "-q", "HEAD"])
            .unwrap_or_default();
        assert_eq!(native::head(repo)?, expected.trim());
        Ok(())
    }
    #[test]
    fn native_refs_cover_unborn_packed_tags_detached_and_tracking() -> Result<(), String> {
        let repo = Repo::new("native-refs-parity")?;
        parity(repo.path())?;
        let base = repo.commit("base", &[("file", Some("one\n"))])?;
        for args in [
            &["branch", "tëst/with-slash"][..],
            &["tag", "main"],
            &["tag", "-a", "annotated", "-m", "tag"],
            &["tag", "-a", "nested", "annotated", "-m", "nested"],
            &["remote", "add", "my/fork", "/a remote path with spaces"],
            &["update-ref", "refs/remotes/my/fork/main", &base],
            &["config", "branch.main.remote", "my/fork"],
            &["config", "branch.main.merge", "refs/heads/main"],
        ] {
            repo.git(args)?;
        }
        parity(repo.path())?;
        repo.commit("ahead", &[])?;
        parity(repo.path())?;
        repo.git(&["checkout", "-q", "--detach", &base])?;
        repo.commit("divergent", &[])?;
        repo.git(&["update-ref", "refs/remotes/my/fork/main", "HEAD"])?;
        repo.git(&["checkout", "-q", "refs/heads/main"])?;
        parity(repo.path())?;
        repo.git(&["pack-refs", "--all"])?;
        parity(repo.path())?;
        repo.git(&["update-ref", "-d", "refs/remotes/my/fork/main"])?;
        parity(repo.path())?;
        Ok(())
    }
    #[test]
    fn ambiguous_upstream_spellings_fall_back_to_git() -> Result<(), String> {
        let repo = Repo::new("native-upstream-ambiguity")?;
        let id = repo.commit("base", &[])?;
        repo.git(&["branch", "origin/main"])?;
        repo.git(&["update-ref", "refs/remotes/origin/main", &id])?;
        repo.git(&["config", "branch.main.remote", "."])?;
        repo.git(&["config", "branch.main.merge", "refs/heads/origin/main"])?;
        assert_eq!(
            native::refs(repo.path()),
            Err("local upstream requires Git".to_owned())
        );
        let expected = super::super::refs::refs_cli(repo.path())?;
        assert_eq!(
            expected
                .local
                .iter()
                .find(|b| b.name == "main")
                .map(|b| b.upstream.as_str()),
            Some("heads/origin/main")
        );
        assert_eq!(super::super::refs(repo.path())?, expected);
        repo.git(&["branch", "-D", "origin/main"])?;
        repo.git(&["remote", "add", "origin", "/a remote"])?;
        repo.git(&["config", "branch.main.remote", "origin"])?;
        repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
        for candidate in [
            "refs/origin/main",
            "refs/tags/origin/main",
            "refs/remotes/origin/main/HEAD",
        ] {
            // A ref cannot coexist with a ref beneath it in the files backend.
            if candidate.ends_with("/HEAD") {
                repo.git(&["update-ref", "-d", "refs/remotes/origin/main"])?;
            }
            repo.git(&["update-ref", candidate, &id])?;
            assert_eq!(
                native::refs(repo.path()),
                Err("ambiguous upstream short name".to_owned()),
                "{candidate}"
            );
            assert_eq!(
                super::super::refs(repo.path())?,
                super::super::refs::refs_cli(repo.path())?
            );
            repo.git(&["update-ref", "-d", candidate])?;
        }
        Ok(())
    }

    #[test]
    fn tracking_walk_budget_is_shared_across_branches() -> Result<(), String> {
        let repo = Repo::new("native-tracking-budget")?;
        let base = repo.commit("base", &[])?;
        repo.commit("one", &[])?;
        repo.commit("two", &[])?;
        repo.git(&["remote", "add", "origin", "/a remote"])?;
        repo.git(&["update-ref", "refs/remotes/origin/main", &base])?;
        repo.git(&["config", "branch.main.remote", "origin"])?;
        repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
        // Three local ancestors and one upstream ancestor fit once.
        assert!(native::refs_bounded(repo.path(), 4).is_ok());
        repo.git(&["branch", "other"])?;
        repo.git(&["config", "branch.other.remote", "origin"])?;
        repo.git(&["config", "branch.other.merge", "refs/heads/main"])?;
        assert_eq!(
            native::refs_bounded(repo.path(), 4),
            Err("tracking graph exceeds native bound".to_owned())
        );
        assert!(native::refs_bounded(repo.path(), 8).is_ok());
        parity(repo.path())?;
        Ok(())
    }

    #[test]
    fn sha256_and_non_commit_tags_match_git() -> Result<(), String> {
        let repo = Repo::new("native-sha256")?;
        std::fs::remove_dir_all(repo.path().join(".git")).map_err(|e| e.to_string())?;
        for args in [
            &["init", "-q", "--object-format=sha256", "-b", "main"][..],
            &["config", "user.name", "Native parity"],
            &["config", "user.email", "native@example.invalid"],
            &["config", "commit.gpgSign", "false"],
        ] {
            repo.git(args)?;
        }
        repo.commit("sha256", &[("file", Some("content\n"))])?;
        let blob = repo.rev("HEAD:file")?;
        let tree = repo.rev("HEAD^{tree}")?;
        repo.git(&["tag", "blob", &blob])?;
        repo.git(&["tag", "-a", "annotated-blob", &blob, "-m", "blob tag"])?;
        repo.git(&["tag", "tree", &tree])?;
        parity(repo.path())?;
        assert_eq!(native::head(repo.path())?.len(), 64);
        Ok(())
    }

    #[test]
    fn included_config_and_config_changes_are_reopened() -> Result<(), String> {
        let repo = Repo::new("native-included-config")?;
        repo.commit("base", &[])?;
        let include = repo.path().join("included config");
        std::fs::write(&include, "[remote \"nested/fork\"]\nurl = /path with spaces\nfetch = +refs/heads/*:refs/remotes/nested/fork/*\n").map_err(|e| e.to_string())?;
        repo.git(&["config", "include.path", &include.to_string_lossy()])?;
        repo.git(&["update-ref", "refs/remotes/nested/fork/main", "HEAD"])?;
        parity(repo.path())?;
        std::fs::write(&include, "[remote \"nested/fork\"]\nurl = https://example.invalid/changed\nfetch = +refs/heads/*:refs/remotes/nested/fork/*\n").map_err(|e| e.to_string())?;
        parity(repo.path())?;
        assert_eq!(
            native::remotes(repo.path())?[0].url,
            "https://example.invalid/changed"
        );
        Ok(())
    }

    #[test]
    fn linked_worktrees_use_their_own_head_and_state_directory() -> Result<(), String> {
        let repo = Repo::new("native-linked-worktree")?;
        repo.commit("base", &[])?;
        let worktree = repo.path().join("linked worktree ü");
        repo.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &worktree.to_string_lossy(),
        ])?;
        parity(&worktree)?;
        assert_ne!(native::git_dir(repo.path())?, native::git_dir(&worktree)?);
        Ok(())
    }
    #[test]
    fn remote_context_cannot_invoke_native_local_reads() {
        let path = Path::new("ssh://host/tmp/repo");
        assert_eq!(capability(path), Err("remote-context"));
        let value = dispatch(
            path,
            Read::Head,
            |_| Err("native must not run".into()),
            || Ok("remote result".to_owned()),
        );
        assert_eq!(value.as_deref(), Ok("remote result"));
    }
    #[test]
    fn control_characters_in_remote_config_retain_cli_display_parsing() -> Result<(), String> {
        let repo = Repo::new("native-remote-display")?;
        for (name, url) in [("tab\tname", "/tmp/path"), ("normal", "/tmp/line\nbreak")] {
            repo.git(&["config", &format!("remote.{name}.url"), url])?;
            assert_eq!(
                native::remotes(repo.path()),
                Err("remote display requires Git".to_owned())
            );
            assert_eq!(
                super::super::remotes(repo.path())?,
                super::super::refs::remotes_cli(repo.path())?
            );
            repo.git(&["config", "--remove-section", &format!("remote.{name}")])?;
        }
        Ok(())
    }

    #[test]
    fn unsupported_and_broken_native_reads_retain_cli_result() -> Result<(), String> {
        let repo = Repo::new("native-fallback")?;
        repo.commit("base", &[])?;
        repo.git(&["remote", "add", "origin", "short:repo"])?;
        repo.git(&["config", "url.https://example.invalid/.insteadOf", "short:"])?;
        assert!(native::remotes(repo.path()).is_err());
        assert_eq!(
            super::super::remotes(repo.path())?,
            super::super::refs::remotes_cli(repo.path())?
        );
        let value = dispatch(
            repo.path(),
            Read::Head,
            |_| Err("simulated corrupt object".into()),
            || Ok("CLI remains authoritative"),
        );
        assert_eq!(value, Ok("CLI remains authoritative"));
        assert!(
            metrics()
                .iter()
                .any(|m| m.operation == "head" && m.fallback_calls > 0)
        );
        Ok(())
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_reference_names_fall_back_without_loss() -> Result<(), String> {
        use std::os::unix::ffi::OsStrExt;
        let repo = Repo::new("native-non-utf8-ref")?;
        let id = repo.commit("base", &[])?;
        let path = repo
            .path()
            .join(".git/refs/heads")
            .join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
        std::fs::write(path, format!("{id}\n")).map_err(|e| e.to_string())?;
        assert!(native::refs(repo.path()).is_err());
        assert_eq!(
            super::super::refs(repo.path())?,
            super::super::refs::refs_cli(repo.path())?
        );
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod executable_context_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn lookup_requires_same_git_before_any_relative_path_entry() -> std::io::Result<()> {
        let root =
            std::env::temp_dir().join(format!("gitcraft-native-git-path-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let first = root.join("first");
        let second = root.join("second");
        let unexecutable = root.join("unexecutable");
        let links = root.join("links");
        for dir in [&first, &second, &unexecutable, &links] {
            std::fs::create_dir_all(dir)?;
        }
        for (dir, mode) in [(&first, 0o755), (&second, 0o755), (&unexecutable, 0o644)] {
            let git = dir.join("git");
            // A marker: the helper must inspect metadata, never execute this file.
            std::fs::write(&git, "This file must never execute")?;
            std::fs::set_permissions(git, std::fs::Permissions::from_mode(mode))?;
        }
        let selected = first.join("git");
        let paths = |dirs: &[&Path]| std::env::join_paths(dirs).map_err(std::io::Error::other);
        let expected = Some(std::fs::canonicalize(&selected)?);
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&first, &second])?),
            expected
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&second, &first])?),
            None
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&unexecutable, &first])?),
            expected
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[Path::new("."), &first])?),
            None
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&first, Path::new("")])?),
            expected
        );
        assert_eq!(
            git_executable_context_matches(
                Some(&selected),
                &paths(&[&first, Path::new("relative")])?
            ),
            expected
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[Path::new(""), &first])?),
            None
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), std::ffi::OsStr::new("")),
            None
        );
        assert_eq!(
            git_executable_context_matches(None, &paths(&[&first])?),
            None
        );
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&links])?),
            None
        );
        symlink(&selected, links.join("git"))?;
        assert_eq!(
            git_executable_context_matches(Some(&selected), &paths(&[&links, &second])?),
            expected
        );
        std::fs::remove_dir_all(root)
    }
}
