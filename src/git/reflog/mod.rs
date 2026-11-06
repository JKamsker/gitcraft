//! Reflogs and lost commits: where HEAD and each branch have been (`git log -g`), and the
//! commits no ref reaches any more, reflogs not counted (`git fsck --unreachable --no-reflogs`).
//! What recovers them (a branch, a cherry-pick, a checkout) is the usual git commands.

mod lost;
mod parse;
#[cfg(test)]
mod tests;

use std::path::Path;

pub use self::lost::{Cancel, LostCommit, lost_commits};
pub use self::parse::{ReflogKind, ReflogMessage, parse_message};

use super::{END_OPTS, NO_SIGNATURE, UTF8, run};

/// How many reflog entries are read at a time; scrolling to the end reads the next page.
pub const REFLOG_PAGE: usize = 200;

/// One entry of a reflog: the ref was moved to `new` (from `old`) at `time`, by `message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflogEntry {
    /// `HEAD@{3}`, `main@{0}`: what git calls the entry.
    pub selector: String,
    /// Unix seconds.
    pub time: i64,
    /// The local date and time, `2024-05-01 14:03`.
    pub date: String,
    /// The commit the ref was moved to.
    pub new: String,
    /// Where it was before: the commit of the entry before it. `None` for the oldest entry (the
    /// ref was made then), and when the entry before isn't read yet.
    pub old: Option<String>,
    /// The parents of `new`.
    pub parents: Vec<String>,
    pub message: ReflogMessage,
    /// The summary of `new`'s message.
    pub summary: String,
}

/// A page of a reflog: `entries`, newest first, and whether older ones follow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReflogPage {
    pub entries: Vec<ReflogEntry>,
    pub more: bool,
}

/// How many fields the format of [`reflog`] has, each ended by a NUL.
const FIELDS: usize = 5;

/// The reflog of `reference` (`HEAD`, `refs/heads/main`), `limit` entries after the first
/// `skip`, newest first. A ref without a reflog (or not there at all, as HEAD in a repository
/// without commits) has an empty one. Entries whose commit is gone (pruned while the reflog
/// kept them) are left out, as git leaves them out.
pub fn reflog(
    repo: &Path,
    reference: &str,
    skip: usize,
    limit: usize,
) -> Result<ReflogPage, String> {
    // One more than asked for: the commit it moved to is where the last one moved from.
    let count = format!("--max-count={}", limit.saturating_add(1));
    let skip_arg = format!("--skip={skip}");
    let read = |date: Option<&str>, format: &str| {
        let mut args = vec!["log", "--walk-reflogs", NO_SIGNATURE, UTF8];
        // Without a date format, `%gd` is numbered: any `--date`, even `default`, dates it.
        args.extend(date);
        args.extend([&skip_arg, &count, format, END_OPTS, reference, "--"]);
        match run(Some(repo), &args) {
            Err(e) if no_reflog(&e) => Ok(String::new()),
            other => other,
        }
    };
    // The date in the selector, and in another read its number: `%gd` gives one or the
    // other. The reflog may move between the two reads; then they are read again.
    for _ in 0..3 {
        let timed = read(
            Some("--date=format-local:%s %Y-%m-%d %H:%M"),
            "--format=%gd%x00%H%x00%P%x00%gs%x00%s%x00",
        )?;
        let numbered = read(None, "--format=%gd%x00%H%x00")?;
        if let Some(entries) = parse_page(&timed, &numbered) {
            return Ok(page(entries, limit));
        }
    }
    Err(format!(
        "The reflog of {} kept changing while it was read; reload to try again",
        short_ref(reference)
    ))
}

/// `refs/heads/main` → `main`.
fn short_ref(reference: &str) -> &str {
    reference.strip_prefix("refs/heads/").unwrap_or(reference)
}

/// Whether git's error says there is nothing to read: a ref that isn't there (a branch deleted
/// meanwhile, HEAD before the first commit).
fn no_reflog(err: &str) -> bool {
    [
        "unknown revision",
        "bad revision",
        "does not have any commits",
    ]
    .iter()
    .any(|s| err.contains(s))
}

/// The entries of the two reads of a page, the dates of one with the numbers of the other.
/// `None` when they aren't of the same entries (the reflog moved between them).
fn parse_page(timed: &str, numbered: &str) -> Option<Vec<ReflogEntry>> {
    let fields: Vec<&str> = timed.split('\0').collect();
    let numbers: Vec<&str> = numbered.split('\0').collect();
    let records = fields.as_chunks::<FIELDS>().0;
    let numbers = numbers.as_chunks::<2>().0;
    if records.len() != numbers.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(records.len());
    for (record, [selector, hash]) in records.iter().zip(numbers) {
        let entry = parse_entry(record, selector.trim_start_matches('\n'))?;
        if entry.new != hash.trim_start_matches('\n') {
            return None;
        }
        entries.push(entry);
    }
    Some(entries)
}

/// One entry: the [`FIELDS`] fields of the dated read, with the selector (`HEAD@{3}`) of the
/// numbered one.
fn parse_entry(record: &[&str; FIELDS], selector: &str) -> Option<ReflogEntry> {
    let [dated, hash, parents, message, summary] = record.map(|f| f.trim_start_matches('\n'));
    let (time, date) = parse::selector_time(dated)?;
    parse::selector_index(selector)?;
    Some(ReflogEntry {
        selector: selector.to_owned(),
        time,
        date,
        new: hash.to_owned(),
        old: None,
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        message: parse_message(message),
        summary: summary.to_owned(),
    })
}

/// The first `limit` of `entries` (one more was read), each with where it moved from: the
/// commit of the entry before it, unless entries between the two were left out (see
/// [`reflog`]), which leaves that unknown.
fn page(mut entries: Vec<ReflogEntry>, limit: usize) -> ReflogPage {
    let more = entries.len() > limit;
    for i in 1..entries.len() {
        let number = |e: &ReflogEntry| parse::selector_index(&e.selector);
        let adjacent = matches!(
            (number(&entries[i - 1]), number(&entries[i])),
            (Some(newer), Some(older)) if older == newer + 1
        );
        if adjacent {
            let before = entries[i].new.clone();
            entries[i - 1].old = Some(before);
        }
    }
    entries.truncate(limit);
    ReflogPage { entries, more }
}
