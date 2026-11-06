//! Changes that make the checks weaker: assertions or tests removed, tests skipped, an
//! assertion that can't fail, a tolerance loosened, snapshots rewritten wholesale, CI allowed
//! to fail or cut down, hooks skipped with `--no-verify`.
//!
//! Read from the diff alone, line by line, the same way every time: each flag says which line
//! made it and why. Nothing is guessed from what the code does.

mod ci;
mod lines;
mod load;
pub mod patch;
mod paths;
mod tolerance;

#[cfg(test)]
pub(crate) mod repo_tests;
#[cfg(test)]
mod tests;

pub use load::{Scope, scan};
use patch::{FilePatch, Hunk, Mark};

/// The badge's label.
pub const BADGE: &str = "weakens checks";

/// Why a file or hunk weakens the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reason {
    RemovedAssertions,
    RemovedTests,
    DeletedTestFile,
    SkipsTests,
    TrivialAssertion,
    LoosenedTolerance,
    SnapshotRewrite,
    CiAllowsFailure,
    CiRemovedJob,
    SkipsHooks,
}

impl Reason {
    /// What it is, as a heading for its evidence.
    pub fn label(self) -> &'static str {
        match self {
            Reason::RemovedAssertions => "Removes assertions",
            Reason::RemovedTests => "Removes tests",
            Reason::DeletedTestFile => "Deletes a test file",
            Reason::SkipsTests => "Skips or disables tests",
            Reason::TrivialAssertion => "Adds an assertion that can't fail",
            Reason::LoosenedTolerance => "Loosens a tolerance",
            Reason::SnapshotRewrite => "Rewrites a snapshot wholesale",
            Reason::CiAllowsFailure => "Lets CI fail and still pass",
            Reason::CiRemovedJob => "Removes a CI job or trigger",
            Reason::SkipsHooks => "Skips git hooks",
        }
    }
}

/// One thing found, with the line that shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flag {
    pub reason: Reason,
    /// Which hunk of the file (by its place, from 0); `None` for the whole file.
    pub hunk: Option<usize>,
    /// The line it is on: in the new file for added lines, in the old one for removed lines.
    pub line: Option<u32>,
    /// What was seen, to show as it is: `line 12: adds #[ignore]`.
    pub evidence: String,
}

/// The flags of one file of a patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFlags {
    pub path: String,
    pub flags: Vec<Flag>,
}

impl FileFlags {
    /// The whole list as a hover text: each reason once, then its evidence.
    pub fn summary(&self) -> String {
        summary(self.flags.iter())
    }
}

/// `flags` as a hover text, grouped by reason.
pub fn summary<'a>(flags: impl Iterator<Item = &'a Flag>) -> String {
    let mut sorted: Vec<&Flag> = flags.collect();
    sorted.sort_by_key(|f| (f.reason, f.line));
    let mut out = String::from("Weakens checks");
    let mut last = None;
    for flag in sorted {
        if last != Some(flag.reason) {
            out.push_str(&format!("\n{}:", flag.reason.label()));
            last = Some(flag.reason);
        }
        out.push_str(&format!("\n  {}", flag.evidence));
    }
    out
}

/// The files of `text` (a diff of one or more files) that weaken the checks.
pub fn scan_patch(text: &str) -> Vec<FileFlags> {
    patch::parse(text)
        .iter()
        .filter_map(|file| {
            let flags = scan_file(file);
            (!flags.is_empty()).then(|| FileFlags {
                path: file.path.clone(),
                flags,
            })
        })
        .collect()
}

/// Longest a line is quoted in the evidence.
const QUOTE_CHARS: usize = 100;

/// `line N: what — the line`, the line cut to [`QUOTE_CHARS`].
fn evidence(line: u32, what: &str, text: &str) -> String {
    let t = text.trim();
    let mut quoted: String = t.chars().take(QUOTE_CHARS).collect();
    if t.chars().count() > QUOTE_CHARS {
        quoted.push('…');
    }
    format!("line {line}: {what}: {quoted}")
}

/// Everything one file's changes weaken.
pub fn scan_file(file: &FilePatch) -> Vec<Flag> {
    let test_file = paths::is_test(&file.path);
    let mut flags = Vec::new();
    if file.deleted && test_file {
        flags.push(Flag {
            reason: Reason::DeletedTestFile,
            hunk: None,
            line: None,
            evidence: format!("deletes {}", file.path),
        });
    }
    if paths::is_snapshot(&file.path) && !file.added {
        flags.extend(snapshot_rewrite(file));
    }
    let ci = paths::ci_kind(&file.path);
    let ci_keys = ci.map(|_| ci::added_keys(&file.hunks)).unwrap_or_default();
    let net_asserts = net(file, lines::is_assertion);
    let net_tests = net(file, lines::is_test_start);
    for (index, hunk) in file.hunks.iter().enumerate() {
        if !file.deleted {
            if net_asserts < 0 {
                flags.extend(removed(
                    hunk,
                    index,
                    lines::is_assertion,
                    Reason::RemovedAssertions,
                ));
            }
            if net_tests < 0 {
                flags.extend(removed(
                    hunk,
                    index,
                    lines::is_test_start,
                    Reason::RemovedTests,
                ));
            }
        }
        flags.extend(added_lines(hunk, index, test_file));
        flags.extend(loosened(hunk, index));
        if let Some(kind) = ci {
            flags.extend(ci::flags(kind, index, hunk, &ci_keys));
        }
    }
    flags
}

/// Added minus removed lines that `is` holds for, over the whole file.
fn net(file: &FilePatch, is: fn(&str) -> bool) -> i64 {
    let count = |mark| {
        file.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.mark == mark && is(&l.text))
            .count() as i64
    };
    count(Mark::Added) - count(Mark::Removed)
}

/// The lines `is` holds for that a hunk removes more of than it adds: each of those removed.
fn removed(hunk: &Hunk, index: usize, is: fn(&str) -> bool, reason: Reason) -> Vec<Flag> {
    let of = |mark| {
        hunk.lines
            .iter()
            .filter(move |l| l.mark == mark && is(&l.text))
    };
    if of(Mark::Removed).count() <= of(Mark::Added).count() {
        return Vec::new();
    }
    of(Mark::Removed)
        .map(|l| Flag {
            reason,
            hunk: Some(index),
            line: Some(l.number),
            evidence: evidence(l.number, "removes", &l.text),
        })
        .collect()
}

/// Skips, assertions that can't fail and skipped hooks among a hunk's added lines.
fn added_lines(hunk: &Hunk, index: usize, test_file: bool) -> Vec<Flag> {
    let mut out = Vec::new();
    for l in hunk.lines.iter().filter(|l| l.mark == Mark::Added) {
        let mut push = |reason, what: &str| {
            out.push(Flag {
                reason,
                hunk: Some(index),
                line: Some(l.number),
                evidence: evidence(l.number, what, &l.text),
            });
        };
        if let Some(marker) = lines::skip_marker(&l.text, test_file) {
            push(Reason::SkipsTests, &format!("adds `{marker}`"));
        }
        if lines::is_trivial_assertion(&l.text) {
            push(Reason::TrivialAssertion, "adds");
        }
        if lines::code(&l.text).is_some() && skips_hooks(&l.text) {
            push(Reason::SkipsHooks, "adds `--no-verify`");
        }
    }
    out
}

fn skips_hooks(text: &str) -> bool {
    text.contains("--no-verify") || text.contains("core.hooksPath=/dev/null")
}

/// Margins a hunk widens: a removed line with a margin, and an added one with the same word
/// and a looser number.
fn loosened(hunk: &Hunk, index: usize) -> Vec<Flag> {
    let margins = |mark| {
        hunk.lines
            .iter()
            .filter(move |l| l.mark == mark && lines::code(&l.text).is_some())
            .filter_map(|l| tolerance::margin(&l.text).map(|m| (l, m)))
    };
    let before: Vec<_> = margins(Mark::Removed).collect();
    margins(Mark::Added)
        .filter_map(|(l, after)| {
            let (_, was) = before.iter().find(|(_, b)| b.word == after.word)?;
            tolerance::loosened(was, &after).then(|| Flag {
                reason: Reason::LoosenedTolerance,
                hunk: Some(index),
                line: Some(l.number),
                evidence: evidence(
                    l.number,
                    &format!("`{}` from {} to {}", after.word, was.value, after.value),
                    &l.text,
                ),
            })
        })
        .collect()
}

/// Fewest changed lines a snapshot needs to count as rewritten wholesale.
const SNAPSHOT_MIN_LINES: usize = 10;

/// A snapshot deleted, or with nearly all its lines replaced (little context left).
fn snapshot_rewrite(file: &FilePatch) -> Option<Flag> {
    let total = |mark| file.hunks.iter().map(|h| h.count(mark)).sum::<usize>();
    let (removed, added, context) = (
        total(Mark::Removed),
        total(Mark::Added),
        total(Mark::Context),
    );
    let what = if file.deleted {
        format!("deletes the snapshot {}", file.path)
    } else if removed >= SNAPSHOT_MIN_LINES
        && added >= SNAPSHOT_MIN_LINES
        && context * 10 <= removed + added
    {
        format!("replaces {removed} lines with {added}, {context} kept")
    } else {
        return None;
    };
    Some(Flag {
        reason: Reason::SnapshotRewrite,
        hunk: None,
        line: None,
        evidence: what,
    })
}
