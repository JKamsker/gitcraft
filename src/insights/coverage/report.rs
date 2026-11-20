//! Reading one coverage report and judging how far it can be trusted for a revision: what the
//! Insights map and the diff's coverage gutter both rest on, so the two never disagree about
//! whether a report is current.
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

/// A report read and judged.
pub(super) struct Loaded {
    pub(super) report: Report,
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
    let path = report_path(repo, name);
    let too_big = |len: u64| {
        format!(
            "{name} is {} MB, more than the {} MB read",
            len >> 20,
            MAX_REPORT >> 20
        )
    };
    if !crate::remote::is_remote(repo)
        && let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > MAX_REPORT
    {
        return Err(too_big(meta.len()));
    }
    let bytes = crate::remote::read(&path).map_err(|e| format!("{name}: {e}"))?;
    if bytes.len() as u64 > MAX_REPORT {
        return Err(too_big(bytes.len() as u64));
    }
    let parsed =
        parse(name, &String::from_utf8_lossy(&bytes)).map_err(|e| format!("{name}: {e}"))?;
    let sidecar = path.with_file_name(format!(
        "{}{SIDECAR}",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let recorded = crate::remote::read_to_string(&sidecar)
        .ok()
        .and_then(|t| read_sidecar(&t));
    let written = modified(&path);
    let edited_after = if *ask == Ask::WorkTree {
        written.and_then(|w| {
            let times: Option<Vec<i64>> = changed.iter().map(|f| modified(&repo.join(f))).collect();
            times.map(|t| t.into_iter().any(|m| m > w))
        })
    } else {
        None
    };
    let freshness = judge(
        recorded.as_deref(),
        ask,
        sha,
        written,
        committed,
        edited_after,
    );
    let found = Found {
        path: name.to_owned(),
        format: parsed.format,
        tests: parsed.tests.clone(),
        per_test: parsed.per_test(),
        files: parsed.files.len(),
        recorded,
        freshness,
        written,
    };
    Ok(Loaded {
        report: parsed,
        found,
    })
}
