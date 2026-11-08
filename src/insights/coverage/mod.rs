//! The test coverage map's analysis (#175 item 11): the lines a change touched (the working
//! tree against HEAD, or a commit against its parent), and what a coverage report says of
//! each: covered, uncovered, not instrumented, or unknown. A report says nothing of the
//! revision it was made at, so it is only taken as matching when a file beside it says so:
//! `<report>.sha` (`lcov.info.sha`), holding the full or abbreviated id of the commit tested.
//! Without one its revision is unknown, and so is what it says of the changed lines: shown as
//! unknown, never as uncovered. Per-test relationships come only from lcov's `TN:` sections,
//! when they name at least two tests; Cobertura records none.
//!
//! Read-only: the report and its `.sha` are read, nothing is written or run.

pub mod cobertura;
pub mod lcov;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use super::{Evidence, Provenance};
use crate::git::{self, Verdict, changed_lines::Hunk};

/// Where reports are looked for, in this order (and `*.lcov` at the top and in `coverage/`).
pub const CONVENTIONAL: [&str; 5] = [
    "lcov.info",
    "coverage/lcov.info",
    "coverage.xml",
    "cobertura.xml",
    "coverage/cobertura-coverage.xml",
];
/// The sidecar recording the commit a report was made at: its name plus this.
pub const SIDECAR: &str = ".sha";
/// Bigger reports are not read.
pub const MAX_REPORT: u64 = 128 << 20;
/// At most so many changed files are mapped.
pub const MAX_FILES: usize = 200;
/// At most so many untracked files count as changed.
pub const MAX_UNTRACKED: usize = 50;
/// At most so many test lane tests are read.
const MAX_LANE_TESTS: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Lcov,
    Cobertura,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Lcov => "lcov",
            Format::Cobertura => "Cobertura",
        }
    }
}

/// What a report says of one file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileCov {
    /// As the report names it.
    pub path: String,
    /// Hits by line, every test together.
    pub lines: BTreeMap<u32, u64>,
    /// The lines each test (by its index in [`Report::tests`]) ran.
    pub tests: BTreeMap<usize, BTreeSet<u32>>,
}

/// A coverage report, parsed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub format: Format,
    pub files: Vec<FileCov>,
    /// The tests it names (lcov `TN:`).
    pub tests: Vec<String>,
    /// The folders its file names are relative to (Cobertura `<source>`).
    pub sources: Vec<String>,
}

impl Report {
    /// Whether it says which test ran which line: only with two tests or more (one name for
    /// everything is no more than the total).
    pub fn per_test(&self) -> bool {
        self.tests.len() >= 2
    }

    /// The report's entry for `path` (relative to the repository at `root`): by its exact
    /// name, or else by a name ending in it (a report made elsewhere, in CI), the second
    /// being `true` then.
    pub fn file(&self, path: &str, root: &str) -> Option<(&FileCov, bool)> {
        let root = normal(root);
        let root = root.trim_end_matches('/');
        let mut by_suffix = None;
        for file in &self.files {
            let name = normal(&file.path);
            let names = std::iter::once(name.clone()).chain(
                self.sources
                    .iter()
                    .map(|s| format!("{}/{name}", normal(s).trim_end_matches('/'))),
            );
            for candidate in names {
                let under_root = (!root.is_empty())
                    .then(|| {
                        candidate
                            .strip_prefix(root)
                            .and_then(|r| r.strip_prefix('/'))
                    })
                    .flatten();
                if under_root == Some(path) || candidate == path {
                    return Some((file, false));
                }
                let absolute = candidate.starts_with('/') || candidate.get(1..2) == Some(":");
                if absolute
                    && under_root.is_none()
                    && candidate.ends_with(&format!("/{path}"))
                    && by_suffix.is_none()
                {
                    by_suffix = Some(file);
                }
            }
        }
        by_suffix.map(|f| (f, true))
    }
}

/// A path with `/` between its names and no leading `./`.
fn normal(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_owned()
}

/// A report's text, parsed as its name or its first character says.
pub fn parse(name: &str, text: &str) -> Result<Report, String> {
    if name.ends_with(".xml") || text.trim_start().starts_with('<') {
        cobertura::parse(text)
    } else {
        lcov::parse(text)
    }
}

/// What the lines are compared between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The working tree (untracked files too) against HEAD.
    WorkTree,
    /// A commit against its first parent.
    Commit(String),
}

/// How far a report can be trusted for the revision asked about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Freshness {
    /// Observed: its sidecar names the commit; inferred: it names HEAD and the uncommitted
    /// edits are older than the report; unknown otherwise.
    pub evidence: Evidence,
    /// Why, in words.
    pub said: String,
    /// Why it is known not to match, when it is.
    pub stale: Option<String>,
}

/// How far a report recording `recorded` (from its sidecar) can be trusted for the lines of
/// `ask` at commit `sha` (HEAD for the working tree), committed at `committed`; the report was
/// written at `written`, and `edited_after` says whether a changed file of the working tree
/// was modified after that (`None`: not known).
pub fn judge(
    recorded: Option<&str>,
    ask: &Ask,
    sha: &str,
    written: Option<i64>,
    committed: i64,
    edited_after: Option<bool>,
) -> Freshness {
    let short = &sha[..sha.len().min(8)];
    let work = *ask == Ask::WorkTree;
    let names_sha = |r: &str| r.len() >= 7 && sha.starts_with(&r.to_ascii_lowercase());
    let freshness = |evidence, said: String, stale: Option<String>| Freshness {
        evidence,
        said,
        stale,
    };
    match recorded {
        Some(r) if names_sha(r) && !work => freshness(
            Evidence::Observed,
            format!("its .sha records {short}, the commit compared"),
            None,
        ),
        Some(r) if names_sha(r) => match edited_after {
            Some(false) => freshness(
                Evidence::Inferred,
                format!(
                    "its .sha records HEAD ({short}) and the uncommitted edits are older than \
                     it: it likely includes them"
                ),
                None,
            ),
            Some(true) => freshness(
                Evidence::Unknown,
                format!("its .sha records HEAD ({short})"),
                Some("files changed after it was written".to_owned()),
            ),
            None => freshness(
                Evidence::Unknown,
                format!(
                    "its .sha records HEAD ({short}); whether it includes the uncommitted \
                     edits can't be told"
                ),
                None,
            ),
        },
        Some(r) => freshness(
            Evidence::Unknown,
            format!("its .sha records {r}"),
            Some(format!("it was made at {r}, not at {short}")),
        ),
        None => {
            let stale = if written.is_some_and(|w| w < committed) {
                Some(format!("written before {short} was committed"))
            } else if work && edited_after == Some(true) {
                Some("files changed after it was written".to_owned())
            } else {
                None
            };
            freshness(
                Evidence::Unknown,
                "revision unknown: no .sha file beside it records the commit tested".to_owned(),
                stale,
            )
        }
    }
}

/// What is known of a changed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCov {
    /// It ran, so many times.
    Covered(u64),
    Uncovered,
    /// The report has nothing for it (a comment, a declaration, a file it doesn't cover).
    NotInstrumented,
    /// No report matches the revision: what it says isn't taken.
    Unknown,
}

impl LineCov {
    pub fn label(self) -> &'static str {
        match self {
            LineCov::Covered(_) => "covered",
            LineCov::Uncovered => "uncovered",
            LineCov::NotInstrumented => "not instrumented",
            LineCov::Unknown => "unknown",
        }
    }
}

/// What a report says of `line` of a file (its entry, if it has one), taken only when the
/// report can be trusted for the revision.
pub fn classify(file: Option<&FileCov>, line: u32, evidence: Evidence) -> LineCov {
    if evidence == Evidence::Unknown {
        return LineCov::Unknown;
    }
    match file.and_then(|f| f.lines.get(&line)) {
        Some(0) => LineCov::Uncovered,
        Some(&n) => LineCov::Covered(n),
        None => LineCov::NotInstrumented,
    }
}

/// A changed line and what is known of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineInfo {
    pub line: u32,
    pub cov: LineCov,
    /// What the report says of it, trusted or not.
    pub hits: Option<u64>,
}

/// A hunk of a changed file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkCov {
    pub hunk: Hunk,
    /// Its lines after the change.
    pub lines: Vec<LineInfo>,
    /// The tests (indexes in the report's) that ran any of its lines.
    pub tests: Vec<usize>,
}

/// How many lines of a hunk (or more) are in each state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub covered: u32,
    pub uncovered: u32,
    pub not_instrumented: u32,
    pub unknown: u32,
}

impl Counts {
    pub fn add(&mut self, other: Counts) {
        self.covered += other.covered;
        self.uncovered += other.uncovered;
        self.not_instrumented += other.not_instrumented;
        self.unknown += other.unknown;
    }
}

impl HunkCov {
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for l in &self.lines {
            match l.cov {
                LineCov::Covered(_) => c.covered += 1,
                LineCov::Uncovered => c.uncovered += 1,
                LineCov::NotInstrumented => c.not_instrumented += 1,
                LineCov::Unknown => c.unknown += 1,
            }
        }
        c
    }

    /// The lines it has after the change, or where it removed them.
    pub fn span(&self) -> (u32, u32) {
        self.hunk
            .new_lines()
            .unwrap_or((self.hunk.new_start.max(1), self.hunk.new_start.max(1)))
    }
}

/// A changed file and its hunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReport {
    pub path: String,
    pub added: bool,
    pub deleted: bool,
    /// Whether the report has it (`None`: no report), and whether by the end of a name.
    pub in_report: Option<bool>,
    pub by_suffix: bool,
    pub hunks: Vec<HunkCov>,
}

/// The report used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// As found or typed (relative to the repository, or absolute).
    pub path: String,
    pub format: Format,
    pub tests: Vec<String>,
    pub per_test: bool,
    pub files: usize,
    /// What its `.sha` holds, if it has one.
    pub recorded: Option<String>,
    pub freshness: Freshness,
    /// When it was written (seconds since the epoch), when that could be told.
    pub written: Option<i64>,
}

/// What a test lane test said of the commit tested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneVerdict {
    pub name: String,
    pub verdict: Verdict,
}

/// The changed lines and what the report says of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    pub ask: Ask,
    pub label: String,
    /// The commit tested: HEAD for the working tree, else the commit.
    pub sha: String,
    /// What it is compared with: HEAD for the working tree, else the commit's parent.
    pub base: String,
    pub report: Option<Found>,
    /// Where reports were looked for, when none was found.
    pub searched: Vec<String>,
    /// The other reports found, to pick from.
    pub others: Vec<String>,
    pub files: Vec<FileReport>,
    /// The test lane's results for the commit tested, and its tree.
    pub lane: Vec<LaneVerdict>,
    pub tree: String,
    pub partial: Vec<String>,
    pub collected: i64,
}

impl Coverage {
    pub fn provenance(&self) -> Provenance {
        let source = match &self.report {
            Some(r) => format!("{} report {}", r.format.label(), r.path),
            None => "no coverage report".to_owned(),
        };
        let mut p = Provenance::new(source, self.label.clone()).tip("tested", self.sha.clone());
        p.collected = self.collected;
        if !self.partial.is_empty() {
            p = p.partial(self.partial.join("; "));
        }
        p.stale = self.report.as_ref().and_then(|r| r.freshness.stale.clone());
        p
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for h in self.files.iter().flat_map(|f| &f.hunks) {
            c.add(h.counts());
        }
        c
    }
}

/// Maps the hunks of a changed file onto the report's entry for it, trusted as `evidence`
/// says.
pub fn map_file(
    path: &str,
    hunks: &[Hunk],
    report: Option<&Report>,
    root: &str,
    evidence: Evidence,
) -> FileReport {
    let entry = report.and_then(|r| r.file(path, root));
    let per_test = report.is_some_and(Report::per_test) && evidence != Evidence::Unknown;
    let hunks = hunks
        .iter()
        .map(|hunk| {
            let lines: Vec<LineInfo> = hunk
                .new_lines()
                .map(|(a, b)| a..=b)
                .into_iter()
                .flatten()
                .map(|line| LineInfo {
                    line,
                    cov: match report {
                        Some(_) => classify(entry.map(|(f, _)| f), line, evidence),
                        None => LineCov::Unknown,
                    },
                    hits: entry.and_then(|(f, _)| f.lines.get(&line).copied()),
                })
                .collect();
            let tests = match entry {
                Some((f, _)) if per_test => f
                    .tests
                    .iter()
                    .filter(|(_, ran)| lines.iter().any(|l| ran.contains(&l.line)))
                    .map(|(&t, _)| t)
                    .collect(),
                _ => Vec::new(),
            };
            HunkCov {
                hunk: *hunk,
                lines,
                tests,
            }
        })
        .collect();
    FileReport {
        path: path.to_owned(),
        added: false,
        deleted: false,
        in_report: report.map(|_| entry.is_some()),
        by_suffix: entry.is_some_and(|(_, s)| s),
        hunks,
    }
}

/// When the file at `path` was last modified, if that can be told here.
fn modified(path: &Path) -> Option<i64> {
    if crate::remote::is_remote(path) {
        return None;
    }
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// Where a report named `name` (relative to the repository, or absolute) is.
fn report_path(repo: &Path, name: &str) -> PathBuf {
    let absolute = Path::new(name).is_absolute() && !crate::remote::is_remote(repo);
    if absolute {
        PathBuf::from(name)
    } else {
        crate::remote::join(repo, name)
    }
}

/// The reports in the conventional places, in order.
pub fn find_reports(repo: &Path) -> Vec<String> {
    let mut found: Vec<String> = CONVENTIONAL
        .iter()
        .filter(|name| crate::remote::try_is_file(&report_path(repo, name)).unwrap_or(false))
        .map(|name| (*name).to_owned())
        .collect();
    // `*.lcov` is looked for only here: listing a folder elsewhere is a round trip per folder.
    if !crate::remote::is_remote(repo) {
        for dir in ["", "coverage"] {
            let Ok(entries) = std::fs::read_dir(repo.join(dir)) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".lcov"))
                .map(|n| {
                    if dir.is_empty() {
                        n
                    } else {
                        format!("{dir}/{n}")
                    }
                })
                .collect();
            names.sort();
            found.extend(names);
        }
    }
    found
}

/// What a report's sidecar records: the commit id it starts with, if it is one.
pub fn read_sidecar(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    (first.len() >= 7 && first.len() <= 64 && first.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| first.to_ascii_lowercase())
}

/// The changed lines of `ask` and what the report `chosen` (or the first found in the
/// conventional places) says of them.
pub fn analyze(repo: &Path, ask: &Ask, chosen: Option<&str>) -> Result<Coverage, String> {
    let mut partial = Vec::new();
    let (sha, base, label, mut files) = match ask {
        Ask::WorkTree => {
            let head = git::resolve(repo, "HEAD")?;
            let mut files = git::changed_lines::read(repo, &head, None, &[])?;
            let untracked = git::run(
                Some(repo),
                &["ls-files", "-z", "--others", "--exclude-standard"],
            )?;
            let untracked: Vec<&str> = untracked.split('\0').filter(|p| !p.is_empty()).collect();
            if untracked.len() > MAX_UNTRACKED {
                partial.push(format!(
                    "only {MAX_UNTRACKED} of {} untracked files are counted",
                    untracked.len()
                ));
            }
            for path in untracked.into_iter().take(MAX_UNTRACKED) {
                let Ok(bytes) = crate::remote::read(&crate::remote::join(repo, path)) else {
                    continue;
                };
                if bytes.contains(&0) {
                    continue;
                }
                let lines = bytes.split(|&b| b == b'\n').count() as u32
                    - u32::from(bytes.ends_with(b"\n") || bytes.is_empty());
                files.push(git::changed_lines::FileHunks {
                    path: path.to_owned(),
                    added: true,
                    hunks: vec![Hunk {
                        old_start: 0,
                        old_count: 0,
                        new_start: 1,
                        new_count: lines,
                    }]
                    .into_iter()
                    .filter(|h| h.new_count > 0)
                    .collect(),
                    ..Default::default()
                });
            }
            let short = &head[..head.len().min(8)];
            (
                head.clone(),
                head.clone(),
                format!("the working tree against HEAD ({short})"),
                files,
            )
        }
        Ask::Commit(rev) => {
            let head = git::resolve(repo, rev)?;
            let base = match git::resolve(repo, &format!("{head}^1")) {
                Ok(parent) => parent,
                Err(_) => {
                    git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")?
                        .trim()
                        .to_owned()
                }
            };
            let files = git::changed_lines::read(repo, &base, Some(&head), &[])?;
            let short = &head[..head.len().min(8)];
            (
                head.clone(),
                base,
                format!("commit {short} against its parent"),
                files,
            )
        }
    };
    files.retain(|f| !f.binary);
    if files.len() > MAX_FILES {
        partial.push(format!(
            "only {MAX_FILES} of {} changed files are mapped",
            files.len()
        ));
        files.truncate(MAX_FILES);
    }

    let found = find_reports(repo);
    let name = chosen.map(str::to_owned).or_else(|| found.first().cloned());
    let searched = if name.is_none() {
        let mut s: Vec<String> = CONVENTIONAL.iter().map(|n| (*n).to_owned()).collect();
        s.push("*.lcov".to_owned());
        s
    } else {
        Vec::new()
    };
    let others = found
        .iter()
        .filter(|f| Some(*f) != name.as_ref())
        .cloned()
        .collect();
    let root = git::run(Some(repo), &["rev-parse", "--show-toplevel"])?
        .trim()
        .to_owned();
    let committed: i64 = git::run(Some(repo), &["log", "-1", "--format=%ct", &sha])?
        .trim()
        .parse()
        .unwrap_or(0);

    let mut report = None;
    let mut found_report = None;
    if let Some(name) = &name {
        let path = report_path(repo, name);
        let bytes = crate::remote::read(&path).map_err(|e| format!("{name}: {e}"))?;
        if bytes.len() as u64 > MAX_REPORT {
            return Err(format!("{name} is bigger than {} MB", MAX_REPORT >> 20));
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
                let times: Option<Vec<i64>> = files
                    .iter()
                    .filter(|f| !f.deleted)
                    .map(|f| modified(&repo.join(&f.path)))
                    .collect();
                times.map(|t| t.into_iter().any(|m| m > w))
            })
        } else {
            None
        };
        let freshness = judge(
            recorded.as_deref(),
            ask,
            &sha,
            written,
            committed,
            edited_after,
        );
        found_report = Some(Found {
            path: name.clone(),
            format: parsed.format,
            tests: parsed.tests.clone(),
            per_test: parsed.per_test(),
            files: parsed.files.len(),
            recorded,
            freshness,
            written,
        });
        report = Some(parsed);
    }
    let evidence = found_report
        .as_ref()
        .map_or(Evidence::Unknown, |f| f.freshness.evidence);
    let files = files
        .iter()
        .map(|f| {
            let mut mapped = map_file(&f.path, &f.hunks, report.as_ref(), &root, evidence);
            mapped.added = f.added;
            mapped.deleted = f.deleted;
            mapped
        })
        .collect();

    let tree = git::run(Some(repo), &["rev-parse", &format!("{sha}^{{tree}}")])?
        .trim()
        .to_owned();
    let lane = lane_verdicts(repo, &tree)?;
    Ok(Coverage {
        ask: ask.clone(),
        label,
        sha,
        base,
        report: found_report,
        searched,
        others,
        files,
        lane,
        tree,
        partial,
        collected: git::now(),
    })
}

/// What the test lane's tests said of `tree`.
fn lane_verdicts(repo: &Path, tree: &str) -> Result<Vec<LaneVerdict>, String> {
    let refs = git::run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", "refs/notes/tests/"],
    )?;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in refs
        .lines()
        .filter_map(|r| r.strip_prefix("refs/notes/tests/"))
        .take(MAX_LANE_TESTS)
    {
        if !seen.insert(name.to_owned()) {
            continue;
        }
        if let Some(&verdict) = git::read_results(repo, name)?.get(tree) {
            out.push(LaneVerdict {
                name: name.to_owned(),
                verdict,
            });
        }
    }
    Ok(out)
}
