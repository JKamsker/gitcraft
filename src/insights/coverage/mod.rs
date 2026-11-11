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

mod mapping;
pub use mapping::{Counts, FileReport, HunkCov, LineCov, map_file};

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

mod read;
pub use read::analyze;
#[cfg(test)]
use read::read_sidecar;
