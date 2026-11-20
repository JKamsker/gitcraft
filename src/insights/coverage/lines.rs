//! What the coverage report says of some lines of one file, as a diff shows them (#173 item
//! 8): the report found the way the Insights map finds it, judged by the same rule
//! ([`judge`]) and classified by the same mapping ([`mapping::classify`]). Nothing here is
//! taken from a report that can't be shown to be of the revision: those lines are unknown.

use super::read::{find_reports, work_tree_files};
use super::report::{self, committed_at, report_files};
use super::*;

/// The side of a diff the lines are numbered on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Side {
    /// A commit's file (`rev` resolves to it).
    Commit(String),
    /// The working tree's file.
    WorkTree,
    /// The index's file: measured lines only when it is what the working tree holds.
    Index,
}

/// What is known of the lines asked about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinesCoverage {
    /// The report used, and how far it is trusted; `None`: no report was found.
    pub found: Option<Found>,
    /// The revision the lines are of: the commit, or HEAD for the working tree and index.
    pub sha: String,
    /// The report has an entry for the file.
    pub in_report: bool,
    /// The file's staged version isn't what the working tree holds (only for [`Side::Index`]),
    /// so the report, measured on the working tree, says nothing of its lines.
    pub index_differs: bool,
    /// Each line asked about, by its number on the side, and what is known of it.
    pub lines: Vec<(u32, LineCov)>,
}

impl LinesCoverage {
    /// How far what it says of the lines is trusted.
    pub fn evidence(&self) -> Evidence {
        match &self.found {
            Some(found) if !self.index_differs => found.freshness.evidence,
            _ => Evidence::Unknown,
        }
    }
}

/// What the first report in the usual places says of `lines` (numbers on `side`) of `path`.
pub fn of_lines(
    repo: &Path,
    side: &Side,
    path: &str,
    lines: &[u32],
) -> Result<LinesCoverage, String> {
    let (ask, rev) = match side {
        Side::Commit(rev) => (Ask::Commit(rev.clone()), rev.as_str()),
        Side::WorkTree | Side::Index => (Ask::WorkTree, "HEAD"),
    };
    let sha = if is_full_id(rev) {
        rev.to_ascii_lowercase()
    } else {
        git::resolve(repo, rev)?
    };
    let reports = find_reports(repo);
    let Some(name) = reports.first() else {
        return Ok(LinesCoverage {
            found: None,
            sha,
            in_report: false,
            index_differs: false,
            lines: lines.iter().map(|&n| (n, LineCov::Unknown)).collect(),
        });
    };
    // The report is parsed again only when its file changed. Git is asked only what can change
    // the verdict ([`judge`]): when the commit was made only matters without a `.sha`, and
    // whether files were edited after the report only when its `.sha` names HEAD.
    let read = report::read(repo, name)?;
    let committed = match read.recorded {
        None => committed_at(repo, &sha)?,
        Some(_) => 0,
    };
    let edited = if ask == Ask::WorkTree && read.names(&sha) {
        let skip = report_files(&reports, Some(name));
        let changed: Vec<String> = work_tree_files(repo, &sha, &mut Vec::new())?
            .into_iter()
            .filter(|f| !f.deleted && !f.binary && !skip.contains(&f.path))
            .map(|f| f.path)
            .collect();
        let changed: Vec<&str> = changed.iter().map(String::as_str).collect();
        report::edited_after(repo, read.written, &changed)
    } else {
        None
    };
    let freshness = judge(
        read.recorded.as_deref(),
        &ask,
        &sha,
        read.written,
        committed,
        edited,
    );
    let index_differs = *side == Side::Index && index_differs(repo, path)?;
    let root = super::cache::toplevel(repo)?;
    let loaded = report::Loaded {
        found: read.found(name, freshness),
        report: read.report,
    };
    let entry = loaded.report.file(path, &root).map(|(f, _)| f);
    let mut out = LinesCoverage {
        found: Some(loaded.found),
        sha,
        in_report: entry.is_some(),
        index_differs,
        lines: Vec::new(),
    };
    let evidence = out.evidence();
    out.lines = lines
        .iter()
        .map(|&n| (n, mapping::classify(entry, n, evidence)))
        .collect();
    Ok(out)
}

/// Whether the working tree's `path` differs from the index's.
fn index_differs(repo: &Path, path: &str) -> Result<bool, String> {
    let literal = format!(":(literal){path}");
    let out = git::run(
        Some(repo),
        &["diff", "--name-only", "-z", "--no-ext-diff", "--", &literal],
    )?;
    Ok(!out.trim_matches('\0').trim().is_empty())
}

/// Whether `rev` is a full commit id already (SHA-1 or SHA-256), which needs no resolving.
fn is_full_id(rev: &str) -> bool {
    matches!(rev.len(), 40 | 64) && rev.bytes().all(|b| b.is_ascii_hexdigit())
}
