//! Backlinks: the commits whose messages mention a commit's hash, in full or abbreviated.
//! Reverts, cherry-picks and trailers (`Fixes: <hash>`) are explicit links; any other
//! mention is a bare one.

use std::path::Path;

use super::{NO_SIGNATURE, UTF8, run};

/// The shortest abbreviation counted as a mention: shorter ones match by chance.
pub const MIN_MENTION: usize = 7;

/// How many mentioning commits are listed at most.
pub const BACKLINK_CAP: usize = 200;

/// How a commit refers to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// `This reverts commit <hash>.`
    Revert,
    /// `(cherry picked from commit <hash>)`
    CherryPick,
    /// A trailer, by its key: `Fixes: <hash> ("subject")`.
    Trailer(String),
    /// Anywhere else in the message.
    Mention,
}

impl LinkKind {
    /// Whether the message means to point at the commit, as opposed to naming it in passing.
    pub fn explicit(&self) -> bool {
        !matches!(self, Self::Mention)
    }

    pub fn label(&self) -> String {
        match self {
            Self::Revert => "Reverts it".to_owned(),
            Self::CherryPick => "Cherry-picked from it".to_owned(),
            Self::Trailer(key) => format!("{key}:"),
            Self::Mention => "Mentions it".to_owned(),
        }
    }
}

/// A later commit that refers to the one shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlink {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub kind: LinkKind,
    /// The line of its message with the hash, trimmed.
    pub text: String,
}

/// The commits mentioning `hash`, the most recent first, at most [`BACKLINK_CAP`]: explicit
/// links and bare mentions, and whether there were more than that.
#[cfg(test)]
pub fn backlinks(repo: &Path, hash: &str) -> Result<(Vec<Backlink>, bool), String> {
    backlinks_scoped(
        repo,
        hash,
        &super::history_scope::default_hidden_namespaces(),
    )
}

pub fn backlinks_scoped(
    repo: &Path,
    hash: &str,
    excluded: &[String],
) -> Result<(Vec<Backlink>, bool), String> {
    let Some(probe) = hash.get(..MIN_MENTION) else {
        return Err(format!("{hash} is not a commit hash"));
    };
    // git finds the messages with the hash's start; which of them really mention it (and how)
    // is read here. Every branch, remote branch and tag; not notes or stashes, whose
    // messages name the commit they were made on.
    let grep = format!("--grep={probe}");
    let revisions = super::history_scope::revision_args(None, excluded);
    let mut args = vec![
        "log",
        NO_SIGNATURE,
        UTF8,
        "--fixed-strings",
        "--regexp-ignore-case",
        &grep,
        "--date=format-local:%Y-%m-%d %H:%M",
        "--format=%H%x00%an%x00%ad%x00%B%x00%x1e",
    ];
    args.extend(revisions.iter().map(String::as_str));
    let out = run(Some(repo), &args)
        .map_err(|e| format!("Could not search the history for mentions: {e}"))?;
    let mut links: Vec<Backlink> = out
        .split('\x1e')
        .filter_map(|record| parse_record(record.trim_start_matches('\n'), hash))
        .collect();
    let more = links.len() > BACKLINK_CAP;
    links.truncate(BACKLINK_CAP);
    Ok((links, more))
}

/// One commit of the search, if its message really mentions `target` (and it isn't itself).
fn parse_record(record: &str, target: &str) -> Option<Backlink> {
    let mut fields = record.splitn(5, '\0');
    let hash = fields.next()?.trim();
    let author = fields.next()?;
    let date = fields.next()?;
    let message = fields.next()?;
    if hash.is_empty() || hash.eq_ignore_ascii_case(target) {
        return None;
    }
    let (kind, text) = find_link(message, target)?;
    Some(Backlink {
        hash: hash.to_owned(),
        subject: message.lines().next().unwrap_or_default().to_owned(),
        author: author.to_owned(),
        date: date.to_owned(),
        kind,
        text,
    })
}

/// How `message` refers to `target`, and the line that does: the most explicit way it does,
/// when it refers to it more than once.
pub(super) fn find_link(message: &str, target: &str) -> Option<(LinkKind, String)> {
    let trailers = trailer_lines(message);
    let mut best: Option<(LinkKind, String)> = None;
    for (n, line) in message.lines().enumerate() {
        if !mentions(line, target) {
            continue;
        }
        let kind = line_kind(line.trim(), trailers.contains(&n));
        let better = match &best {
            None => true,
            Some((have, _)) => rank(&kind) > rank(have),
        };
        if better {
            best = Some((kind, clip(line.trim())));
        }
    }
    best
}

/// Explicit links first, a revert before a cherry-pick before a trailer.
fn rank(kind: &LinkKind) -> u8 {
    match kind {
        LinkKind::Revert => 3,
        LinkKind::CherryPick => 2,
        LinkKind::Trailer(_) => 1,
        LinkKind::Mention => 0,
    }
}

/// What kind of link `line` (one that mentions the hash) is; `in_trailers`: it is in the
/// message's trailer block.
fn line_kind(line: &str, in_trailers: bool) -> LinkKind {
    let lower = line.to_lowercase();
    if lower.starts_with("this reverts commit ") {
        LinkKind::Revert
    } else if is_cherry_pick_line(&lower) {
        LinkKind::CherryPick
    } else if let Some(key) = trailer_key(line).filter(|_| in_trailers) {
        LinkKind::Trailer(key.to_owned())
    } else {
        LinkKind::Mention
    }
}

fn is_cherry_pick_line(lower: &str) -> bool {
    lower.starts_with("(cherry picked from commit ")
}

/// The numbers of the lines of `message` in its trailer block: its last paragraph, not the
/// subject, when every line of it is a trailer (`Key: value`), the continuation of one, or
/// the line `cherry-pick -x` adds. A `fix: …` subject or a `Note: …` line in the body is
/// none.
fn trailer_lines(message: &str) -> Vec<usize> {
    let lines: Vec<&str> = message.lines().collect();
    let Some(last) = lines.iter().rposition(|l| !l.trim().is_empty()) else {
        return Vec::new();
    };
    let start = lines[..last]
        .iter()
        .rposition(|l| l.trim().is_empty())
        .map_or(0, |blank| blank + 1);
    // The subject's paragraph is never the trailers.
    if start == 0 {
        return Vec::new();
    }
    let block = start..=last;
    let all_trailers = lines[block.clone()].iter().enumerate().all(|(i, line)| {
        let continuation = i > 0 && line.starts_with([' ', '\t']);
        continuation
            || trailer_key(line.trim()).is_some()
            || is_cherry_pick_line(&line.trim().to_lowercase())
    });
    if all_trailers {
        block.collect()
    } else {
        Vec::new()
    }
}

/// The key of a trailer line (`Fixes` of `Fixes: abc…`): letters, digits and dashes, then a
/// colon and a space.
fn trailer_key(line: &str) -> Option<&str> {
    let (key, rest) = line.split_once(':')?;
    let ok = !key.is_empty()
        && key.len() <= 40
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && rest.starts_with(' ');
    // A link (`https://…/commit/<hash>`) has no space after its colon: no trailer.
    ok.then_some(key)
}

/// Whether `text` has a word that is `target` or an abbreviation of it at least
/// [`MIN_MENTION`] long (a word: hex digits with no letter or digit right before or after).
pub(super) fn mentions(text: &str, target: &str) -> bool {
    let target = target.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_hexdigit() || !word_starts(&text[..i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        let word_ends = text[i..].chars().next().is_none_or(|c| !is_word_char(c));
        let word = &text[start..i];
        if word_ends
            && word.len() >= MIN_MENTION
            && word.len() <= target.len()
            && target.starts_with(&word.to_ascii_lowercase())
        {
            return true;
        }
    }
    false
}

/// Whether a word can start after `before`: at the start, after a character that isn't part
/// of a word, or after the `-g` of a `git describe` name (`v1.0-3-g0123456`).
fn word_starts(before: &str) -> bool {
    let mut back = before.chars().rev();
    match back.next() {
        None => true,
        Some('g') => back.next() == Some('-'),
        Some(c) => !is_word_char(c),
    }
}

/// Letters, digits and `_`: what a hash would run into, were it part of a longer word. Not
/// the ideographs and kana of Chinese and Japanese, written without spaces between words.
fn is_word_char(c: char) -> bool {
    c == '_' || (c.is_alphanumeric() && !is_cjk(c))
}

fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x2FFF | 0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF
    )
}

/// `line` cut to a length that fits a row, on a character boundary.
fn clip(line: &str) -> String {
    const MAX: usize = 200;
    if line.chars().count() <= MAX {
        line.to_owned()
    } else {
        let mut cut: String = line.chars().take(MAX).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn short_and_full_hashes_are_mentions_and_lookalikes_are_not() {
        assert!(mentions("see 0123456", H));
        assert!(mentions("see 0123456789AB.", H));
        assert!(mentions(H, H));
        assert!(mentions("(0123456789)", H));
        assert!(mentions("Zoë fixed 0123456 ✓", H));
        // Too short, diverging, part of a longer word.
        assert!(!mentions("see 012345", H));
        assert!(!mentions("see 0123457", H));
        assert!(!mentions("x0123456", H));
        assert!(!mentions("0123456g", H));
        assert!(!mentions("é0123456", H));
        assert!(!mentions(&format!("{H}0"), H));
        assert!(!mentions("", H));
        // Beside non-ASCII punctuation, spaces and ideographs, or after describe's `-g`.
        assert!(mentions("Reverts “0123456”", H));
        assert!(mentions("修复0123456引入的问题", H));
        assert!(mentions("see\u{a0}0123456", H));
        assert!(mentions("since v1.0-3-g0123456", H));
        assert!(!mentions("big0123456", H));
        assert!(!mentions("ü0123456", H));
    }

    #[test]
    fn only_the_last_paragraph_of_trailers_holds_trailers() {
        let kind = |m: &str| find_link(m, H).map(|(k, _)| k);
        // A conventional-commit subject and a `Note:` line in the body are mentions.
        assert_eq!(
            kind("fix: regression from 0123456"),
            Some(LinkKind::Mention)
        );
        assert_eq!(
            kind("x\n\nNote: 0123456 broke this.\n\nSigned-off-by: a <a@a>"),
            Some(LinkKind::Mention)
        );
        // A trailer block with a continuation and cherry-pick's line.
        assert_eq!(
            kind(
                "x\n\nbody\n\nFixes: 0123456 (\"the\n  subject\")\n(cherry picked from commit fedcba9)\nSigned-off-by: a <a@a>"
            ),
            Some(LinkKind::Trailer("Fixes".to_owned()))
        );
        // A last paragraph that isn't all trailers.
        assert_eq!(
            kind("x\n\nFixes: 0123456\nand some prose"),
            Some(LinkKind::Mention)
        );
    }

    #[test]
    fn explicit_links_are_told_from_mentions() {
        let kind = |m: &str| find_link(m, H).map(|(k, _)| k);
        assert_eq!(
            kind(&format!("Revert \"x\"\n\nThis reverts commit {H}.")),
            Some(LinkKind::Revert)
        );
        assert_eq!(
            kind("x\n\n(cherry picked from commit 0123456789abcdef)"),
            Some(LinkKind::CherryPick)
        );
        assert_eq!(
            kind("x\n\nFixes: 0123456789ab (\"the bug\")"),
            Some(LinkKind::Trailer("Fixes".to_owned()))
        );
        assert_eq!(kind("follow-up to 0123456"), Some(LinkKind::Mention));
        assert_eq!(
            kind("https://example.org/commit/0123456789"),
            Some(LinkKind::Mention)
        );
        assert_eq!(kind("nothing here"), None);
        // The most explicit line wins, its text is the one kept.
        let both = format!("Fix 0123456 again\n\nThis reverts commit {H}.");
        assert_eq!(
            find_link(&both, H),
            Some((LinkKind::Revert, format!("This reverts commit {H}.")))
        );
        assert!(LinkKind::Trailer("Refs".into()).explicit());
        assert!(!LinkKind::Mention.explicit());
    }

    #[test]
    fn records_skip_the_commit_itself_and_keep_the_subject() {
        let me = format!("{H}\0a\0d\0Mentions {H}\0");
        assert_eq!(parse_record(&me, H), None);
        let other = "fedcba9876543210fedcba9876543210fedcba98\0Zoë\x002026-01-01 10:00\0Revert \"x\"\n\nThis reverts commit 0123456789ab.\n\0";
        let link = parse_record(other, H);
        assert_eq!(
            link.as_ref()
                .map(|l| (&l.subject[..], &l.author[..], &l.kind)),
            Some(("Revert \"x\"", "Zoë", &LinkKind::Revert))
        );
        assert_eq!(parse_record("", H), None);
        assert_eq!(parse_record("garbage", H), None);
    }

    #[test]
    fn a_huge_line_is_clipped_on_a_character_boundary() {
        let line = format!("{} 0123456 {}", "é".repeat(300), "ü".repeat(300));
        let (_, text) = find_link(&line, H).unwrap_or((LinkKind::Mention, String::new()));
        assert_eq!(text.chars().count(), 201);
        assert!(text.ends_with('…'));
    }
}
