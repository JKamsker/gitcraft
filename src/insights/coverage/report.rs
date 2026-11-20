//! Reading one coverage report and judging how far it can be trusted for a revision: what the
//! Insights map and the diff's coverage gutter both rest on, so the two never disagree about
//! whether a report is current.
use std::sync::Arc;

use super::read::read_sidecar;
use super::*;

/// When the file at `path` was last modified, if that can be told here.
pub(super) fn modified(path: &Path) -> Option<i64> {
    if crate::remote::is_remote(path) {
        return None;
    }
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// Where a report named `name` (relative to the repository, or absolute on the machine the
/// repository is on) is.
pub(super) fn report_path(repo: &Path, name: &str) -> PathBuf {
    if crate::remote::is_remote(repo) {
        crate::remote::resolve(repo, name)
    } else if Path::new(name).is_absolute() {
        PathBuf::from(name)
    } else {
        repo.join(name)
    }
}

/// When commit `sha` was committed (seconds since the epoch; 0 when git doesn't say).
pub(super) fn committed_at(repo: &Path, sha: &str) -> Result<i64, String> {
    Ok(git::run(Some(repo), &["log", "-1", "--format=%ct", sha])?
        .trim()
        .parse()
        .unwrap_or(0))
}

/// The reports `found` and the one picked (`name`), with their `.sha` files: not changes to
/// look at, even when they show up as untracked files.
pub(super) fn report_files(found: &[String], name: Option<&String>) -> HashSet<String> {
    found
        .iter()
        .chain(name)
        .flat_map(|r| [r.clone(), format!("{r}{SIDECAR}")])
        .collect()
}

/// A report as read, before it is judged for a revision.
pub(super) struct Read {
    pub(super) report: Arc<Report>,
    /// What its `.sha` holds, if it has one.
    pub(super) recorded: Option<String>,
    /// When it was written (seconds since the epoch), when that could be told.
    pub(super) written: Option<i64>,
}

impl Read {
    /// Whether its `.sha` names commit `sha`, as [`judge`] takes it.
    pub(super) fn names(&self, sha: &str) -> bool {
        self.recorded
            .as_deref()
            .is_some_and(|r| r.len() >= 7 && sha.starts_with(r))
    }

    /// What it says it is, judged `freshness`.
    pub(super) fn found(&self, name: &str, freshness: Freshness) -> Found {
        Found {
            path: name.to_owned(),
            format: self.report.format,
            tests: self.report.tests.clone(),
            per_test: self.report.per_test(),
            files: self.report.files.len(),
            recorded: self.recorded.clone(),
            freshness,
            written: self.written,
        }
    }
}

/// Reads the report `name` (parsed again only when its file changed, see [`cache`]) and its
/// `.sha`.
pub(super) fn read(repo: &Path, name: &str) -> Result<Read, String> {
    let path = report_path(repo, name);
    let report = super::cache::parsed(repo, &path, name)?;
    let sidecar = path.with_file_name(format!(
        "{}{SIDECAR}",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let recorded = crate::remote::read_to_string(&sidecar)
        .ok()
        .and_then(|t| read_sidecar(&t));
    Ok(Read {
        report,
        recorded,
        written: modified(&path),
    })
}

/// Whether one of the working tree's `changed` files (that still exist) was modified after a
/// report written at `written` (`None`: not known).
pub(super) fn edited_after(repo: &Path, written: Option<i64>, changed: &[&str]) -> Option<bool> {
    written.and_then(|w| {
        let times: Option<Vec<i64>> = changed.iter().map(|f| modified(&repo.join(f))).collect();
        times.map(|t| t.into_iter().any(|m| m > w))
    })
}

/// A report read and judged.
pub(super) struct Loaded {
    pub(super) report: Arc<Report>,
    pub(super) found: Found,
}

/// Reads the report `name` and judges it for the lines of `ask` at commit `sha` (HEAD for the
/// working tree), committed at `committed`. `changed`: the working tree's changed files that
/// still exist (only asked about for [`Ask::WorkTree`]), to tell whether one was edited after
/// the report was written.
pub(super) fn read_report(
    repo: &Path,
    name: &str,
    ask: &Ask,
    sha: &str,
    committed: i64,
    changed: &[&str],
) -> Result<Loaded, String> {
    let read = read(repo, name)?;
    let edited = (*ask == Ask::WorkTree)
        .then(|| edited_after(repo, read.written, changed))
        .flatten();
    let freshness = judge(
        read.recorded.as_deref(),
        ask,
        sha,
        read.written,
        committed,
        edited,
    );
    Ok(Loaded {
        found: read.found(name, freshness),
        report: read.report,
    })
}
