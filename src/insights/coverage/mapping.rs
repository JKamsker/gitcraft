//! Map coverage evidence onto changed files, hunks and lines.
use super::*;

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
