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

pub use load::{Area, Scope, scan};
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
            Reason::SkipsTests => "Skips, disables or focuses tests",
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
    /// The line is a removed one (its number is in the old file).
    pub removed: bool,
    /// What was seen, to show as it is: `line 12: #[ignore]`, `old line 4: assert x`.
    pub evidence: String,
}

impl Flag {
    /// A flag for `line` of hunk `hunk`, with `detail` (what changed) before the line quoted.
    fn at(reason: Reason, hunk: usize, line: &patch::Line, detail: Option<String>) -> Self {
        let removed = line.mark == Mark::Removed;
        let side = if removed { "old line" } else { "line" };
        let detail = detail.map(|d| format!(" ({d})")).unwrap_or_default();
        Flag {
            reason,
            hunk: Some(hunk),
            line: Some(line.number),
            removed,
            evidence: format!("{side} {}{detail}: {}", line.number, quote(&line.text)),
        }
    }

    /// A flag for the whole file.
    fn file(reason: Reason, evidence: String) -> Self {
        Flag {
            reason,
            hunk: None,
            line: None,
            removed: false,
            evidence,
        }
    }
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

/// Most lines of evidence a hover lists per reason.
const LISTED: usize = 5;

/// `flags` as a hover text, grouped by reason, removed lines before added ones.
pub fn summary<'a>(flags: impl Iterator<Item = &'a Flag>) -> String {
    let mut sorted: Vec<&Flag> = flags.collect();
    sorted.sort_by_key(|f| (f.reason, !f.removed, f.line));
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while at < sorted.len() {
        let reason = sorted[at].reason;
        let same = sorted[at..]
            .iter()
            .take_while(|f| f.reason == reason)
            .count();
        out.push(format!("{}:", reason.label()));
        for flag in &sorted[at..at + same.min(LISTED)] {
            out.push(format!("  {}", flag.evidence));
        }
        if same > LISTED {
            out.push(format!("  and {} more", same - LISTED));
        }
        at += same;
    }
    out.join("\n")
}

/// The files of `text` (a diff of one or more files) that weaken the checks.
pub fn scan_patch(text: &str) -> Vec<FileFlags> {
    let files = patch::parse(text);
    let moved = moved_lines(&files);
    files
        .iter()
        .filter_map(|file| {
            let flags = scan_file(file, &moved);
            (!flags.is_empty()).then(|| FileFlags {
                path: file.path.clone(),
                flags,
            })
        })
        .collect()
}

/// Longest a line is quoted in the evidence.
const QUOTE_CHARS: usize = 100;

/// The line trimmed and cut to [`QUOTE_CHARS`].
fn quote(text: &str) -> String {
    let t = text.trim();
    let mut quoted: String = t.chars().take(QUOTE_CHARS).collect();
    if t.chars().count() > QUOTE_CHARS {
        quoted.push('…');
    }
    quoted
}

/// Files whose lines are prose, not code: what they say about tests or hooks is no change.
fn is_prose(path: &str) -> bool {
    let lower = path.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    matches!(
        ext,
        "md" | "markdown" | "txt" | "rst" | "adoc" | "asciidoc" | "org" | "textile"
    )
}

/// How `path`'s lines are read.
fn lang_of(path: &str) -> lines::Lang {
    let lower = path.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    lines::Lang {
        test_file: paths::is_test(path),
        dash_comments: matches!(ext, "sql" | "lua" | "hs" | "elm" | "ada"),
    }
}

/// Assertions and tests added somewhere in the patch, as they read trimmed: removed from one
/// file and added to another, they moved and weaken nothing.
fn moved_lines(files: &[FilePatch]) -> std::collections::HashSet<String> {
    files
        .iter()
        .flat_map(|f| {
            let lang = lang_of(&f.path);
            f.hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(move |l| {
                    l.mark == Mark::Added
                        && (lines::is_assertion(&l.text, lang)
                            || lines::is_test_start(&l.text, lang))
                })
                .map(|l| l.text.trim().to_owned())
        })
        .collect()
}

/// Everything one file's changes weaken; `moved`: lines added elsewhere in the patch.
pub fn scan_file(file: &FilePatch, moved: &std::collections::HashSet<String>) -> Vec<Flag> {
    let lang = lang_of(&file.path);
    let mut flags = Vec::new();
    if file.deleted && lang.test_file {
        flags.push(Flag::file(
            Reason::DeletedTestFile,
            format!("deletes {}", file.path),
        ));
    }
    if paths::is_snapshot(&file.path) && !file.added {
        flags.extend(snapshot_rewrite(file));
    }
    if is_prose(&file.path) {
        return flags;
    }
    let ci = paths::ci_kind(&file.path);
    let ci_keys = ci.map(|_| ci::added_keys(&file.hunks)).unwrap_or_default();
    let ci_file = ci.map(|kind| ci::FileCi {
        kind,
        added_keys: &ci_keys,
        job_indent: ci::job_indent(&file.hunks),
    });
    let assertion = Kind {
        is: lines::is_assertion,
        lang,
        moved,
    };
    let test = Kind {
        is: lines::is_test_start,
        lang,
        moved,
    };
    let net_asserts = net(file, &assertion);
    let net_tests = net(file, &test);
    for (index, hunk) in file.hunks.iter().enumerate() {
        if !file.deleted {
            if net_asserts < 0 {
                flags.extend(removed(hunk, index, &assertion, Reason::RemovedAssertions));
            }
            if net_tests < 0 {
                flags.extend(removed(hunk, index, &test, Reason::RemovedTests));
            }
        }
        flags.extend(added_lines(hunk, index, lang, ci.is_some()));
        flags.extend(loosened(hunk, index, lang));
        if let Some(ci_file) = &ci_file {
            flags.extend(ci::flags(ci_file, index, hunk));
        }
    }
    flags
}

/// A kind of line counted (assertions, tests): a removed one added back somewhere in the
/// patch (`moved`) is not counted as removed.
struct Kind<'a> {
    is: fn(&str, lines::Lang) -> bool,
    lang: lines::Lang,
    moved: &'a std::collections::HashSet<String>,
}

impl Kind<'_> {
    fn counts(&self, line: &patch::Line) -> bool {
        match line.mark {
            Mark::Added => (self.is)(&line.text, self.lang),
            Mark::Removed => {
                (self.is)(&line.text, self.lang) && !self.moved.contains(line.text.trim())
            }
            Mark::Context => false,
        }
    }
}

/// Added minus removed lines of `kind`, over the whole file.
fn net(file: &FilePatch, kind: &Kind<'_>) -> i64 {
    let count = |mark| {
        file.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.mark == mark && kind.counts(l))
            .count() as i64
    };
    count(Mark::Added) - count(Mark::Removed)
}

/// The lines of `kind` that a hunk removes more of than it adds: each of those removed.
fn removed(hunk: &Hunk, index: usize, kind: &Kind<'_>, reason: Reason) -> Vec<Flag> {
    let of = |mark| {
        hunk.lines
            .iter()
            .filter(move |l| l.mark == mark && kind.counts(l))
    };
    if of(Mark::Removed).count() <= of(Mark::Added).count() {
        return Vec::new();
    }
    of(Mark::Removed)
        .map(|l| Flag::at(reason, index, l, None))
        .collect()
}

/// Skips, assertions that can't fail and skipped hooks among a hunk's added lines.
fn added_lines(hunk: &Hunk, index: usize, lang: lines::Lang, ci: bool) -> Vec<Flag> {
    let mut out = Vec::new();
    for l in hunk.lines.iter().filter(|l| l.mark == Mark::Added) {
        if lines::skip_marker(&l.text, lang).is_some() {
            out.push(Flag::at(Reason::SkipsTests, index, l, None));
        }
        if lines::is_trivial_assertion(&l.text, lang) {
            out.push(Flag::at(Reason::TrivialAssertion, index, l, None));
        }
        // Skipping hooks is said on a command line: a continuation line (`  --no-verify \`) of
        // a script or a CI step counts, as code elsewhere does.
        let text = l.text.trim_start();
        let hook_line = lines::code(&l.text, lang).is_some() || (ci && text.starts_with("--"));
        if hook_line && skips_hooks(text) {
            out.push(Flag::at(Reason::SkipsHooks, index, l, None));
        }
    }
    out
}

fn skips_hooks(text: &str) -> bool {
    text.contains("--no-verify") || text.contains("core.hooksPath=/dev/null")
}

/// Margins a hunk widens. The lines with a margin it removes and those it adds are taken in
/// pairs, in order (the first removed with the first added…), and each word compared.
/// Only in test files, or on a line that asserts.
fn loosened(hunk: &Hunk, index: usize, lang: lines::Lang) -> Vec<Flag> {
    let margins = |mark| {
        hunk.lines
            .iter()
            .filter(move |l| l.mark == mark && lines::code(&l.text, lang).is_some())
            .filter(move |l| lang.test_file || lines::is_assertion(&l.text, lang))
            .map(|l| (l, tolerance::margins(&l.text)))
            .filter(|(_, m)| !m.is_empty())
    };
    margins(Mark::Removed)
        .zip(margins(Mark::Added))
        .flat_map(|((_, before), (line, after))| {
            tolerance::loosened(&before, &after)
                .into_iter()
                .map(|(m, was)| {
                    let detail = format!("{} {was} → {}", m.word.trim_end_matches('('), m.written);
                    Flag::at(Reason::LoosenedTolerance, index, line, Some(detail))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Fewest changed lines a snapshot needs to count as rewritten wholesale.
const SNAPSHOT_MIN_LINES: usize = 10;

/// A snapshot deleted, or with nine lines in ten or more replaced: one hunk from its first
/// line to its end (fewer lines of context after it than git shows, so nothing follows).
fn snapshot_rewrite(file: &FilePatch) -> Option<Flag> {
    if file.deleted {
        return Some(Flag::file(
            Reason::SnapshotRewrite,
            format!("deletes the snapshot {}", file.path),
        ));
    }
    let [hunk] = file.hunks.as_slice() else {
        return None;
    };
    let removed = hunk.count(Mark::Removed);
    let trailing = hunk
        .lines
        .iter()
        .rev()
        .take_while(|l| l.mark == Mark::Context)
        .count();
    let old = hunk.old_count as usize;
    let whole = hunk.old_start <= 1 && trailing < 3 && removed * 10 >= old * 9;
    (removed >= SNAPSHOT_MIN_LINES && whole).then(|| {
        Flag::file(
            Reason::SnapshotRewrite,
            format!("replaces {removed} of its {old} lines"),
        )
    })
}
