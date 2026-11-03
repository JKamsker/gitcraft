//! One pull request in full, whichever forge it's on: its description, build status, commits and
//! comments, and the changes it proposes. The forges' modules (`forge/`) read it; this is what they
//! read it into, and what the page makes of it.

use std::path::Path;

use serde_json::Value;

use crate::forge::{self, Forge};
use crate::git::{self, ChangedFile, Credentials, Remote};
use crate::pr_threads::PrExtras;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Failed,
    Running,
    /// Skipped, cancelled by a newer run, neutral: neither good nor bad.
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    /// The workflow it belongs to or the status's description; may be empty.
    pub detail: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCommit {
    pub oid: String,
    pub headline: String,
    pub author: String,
}

/// Which version of a line a comment is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiffSide {
    /// As it was: a removed line.
    Left,
    /// As it is now: an added line, or one that didn't change.
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Comment,
    /// What the review said: "approved", "requested changes"…
    Review(&'static str),
    /// On a line of a file.
    Inline {
        path: String,
        line: Option<u64>,
        side: DiffSide,
        /// The line has changed since, so it isn't where the comment says any more.
        outdated: bool,
        resolved: bool,
    },
}

/// One thing said on the pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub author: String,
    pub at: i64,
    pub body: String,
    pub url: String,
    /// The review this is, if it can be dismissed.
    pub review_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrDetails {
    /// The forge it was read from.
    pub forge: Option<Forge>,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub url: String,
    pub base_branch: String,
    pub base_oid: String,
    pub head_branch: String,
    pub head_oid: String,
    /// Where the forge keeps the proposed commits (`refs/pull/12/head`), to fetch them by.
    pub head_ref: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub mergeable: String,
    pub merge_state: String,
    pub review_decision: String,
    pub labels: Vec<String>,
    pub checks: Vec<Check>,
    /// The checks couldn't be read: why. Without it, no checks means none ran.
    pub checks_error: Option<String>,
    pub commits: Vec<PrCommit>,
    /// Oldest first.
    pub timeline: Vec<Entry>,
    /// The comments on lines couldn't be read: why.
    pub inline_error: Option<String>,
    /// Threads on lines and what the person logged in may do.
    pub extras: PrExtras,
}

/// The sent comments of the threads, for the conversation.
pub(crate) fn line_entries(extras: &PrExtras) -> Vec<Entry> {
    extras
        .threads
        .iter()
        .flat_map(|thread| {
            thread
                .comments
                .iter()
                .filter(|comment| !comment.pending)
                .map(move |comment| Entry {
                    kind: EntryKind::Inline {
                        path: thread.path.clone(),
                        line: thread.line,
                        side: thread.side,
                        outdated: thread.outdated,
                        resolved: thread.resolved,
                    },
                    author: comment.author.clone(),
                    at: comment.at,
                    body: comment.body.clone(),
                    url: comment.url.clone(),
                    review_id: None,
                })
        })
        .collect()
}

/// What the page says about the reviews so far.
pub fn reviews_label(decision: &str, open: bool, timeline: &[Entry]) -> &'static str {
    let reviewed = timeline
        .iter()
        .any(|entry| matches!(entry.kind, EntryKind::Review(_) | EntryKind::Inline { .. }));
    match decision {
        "APPROVED" => "Approved",
        "CHANGES_REQUESTED" => "Changes requested",
        "REVIEW_REQUIRED" => "Review required",
        _ if reviewed => "Reviewed",
        _ if open => "No reviews yet",
        _ => "No reviews",
    }
}

/// The changes a pull request proposes, as its web page shows them: from the merge base.
#[derive(Clone, Debug)]
pub struct PrFiles {
    pub from: String,
    pub to: String,
    pub files: Vec<ChangedFile>,
}

/// Finds the changes of `details`, fetching its commits first if they aren't here yet.
pub fn files(
    repo: &Path,
    details: &PrDetails,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
) -> Result<PrFiles, String> {
    let has = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if !has(&details.head_oid) || !has(&details.base_oid) {
        let remote = remote_for(remotes, &details.url)
            .ok_or("no remote of this repository is the one the pull request belongs to")?;
        // No refs and no FETCH_HEAD: nothing here changes but the objects being available.
        let base = format!("refs/heads/{}", details.base_branch);
        let mut args = vec!["fetch", "--no-tags", "--no-write-fetch-head", remote];
        if !details.head_ref.is_empty() {
            args.push(&details.head_ref);
        }
        args.push(&base);
        // The branches may have moved on since the details were read (a force push even leaves
        // the old commits out), so the very commits are asked for too.
        args.extend(
            [&details.head_oid, &details.base_oid]
                .into_iter()
                .filter(|oid| !oid.is_empty())
                .map(String::as_str),
        );
        git::run_with(Some(repo), &args, credentials)?;
    }
    let from = git::merge_base(repo, &details.base_oid, &details.head_oid)?;
    let files = git::diff_files(repo, &from, Some(&details.head_oid))?;
    Ok(PrFiles {
        from,
        to: details.head_oid.clone(),
        files,
    })
}

/// The string at `pointer`, or nothing.
pub(crate) fn at<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The array at `pointer`, or nothing.
pub(crate) fn items<'a>(v: &'a Value, pointer: &str) -> &'a [Value] {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Seconds since the epoch of a time like `2026-11-02T20:37:34Z` or `2026-11-02T21:37:34+01:00`.
pub(crate) fn parse_time(iso: &str) -> Option<i64> {
    let (iso, offset) = match iso.strip_suffix('Z') {
        Some(utc) => (utc, 0),
        None => {
            let at = iso.len().checked_sub(6)?;
            let (time, zone) = (iso.get(..at)?, iso.get(at..)?);
            let sign = match zone.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return None,
            };
            let (hours, minutes) = zone.get(1..)?.split_once(':')?;
            let offset = hours.parse::<i64>().ok()? * 3_600 + minutes.parse::<i64>().ok()? * 60;
            (time, sign * offset)
        }
    };
    let (date, time) = iso.split_once('T')?;
    let numbers = |text: &str, separator: char| -> Option<[i64; 3]> {
        let mut parts = text.split(separator).map(|n| n.parse::<i64>().ok());
        Some([parts.next()??, parts.next()??, parts.next()??])
    };
    let [year, month, day] = numbers(date, '-')?;
    let [hour, minute, second] = numbers(time.split('.').next()?, ':')?;
    // Days from the civil calendar, counted from the March before: leap days come last then.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

/// How many checks passed, failed, are running and were skipped.
pub fn tally(checks: &[Check]) -> (usize, usize, usize, usize) {
    let count = |state| checks.iter().filter(|c| c.state == state).count();
    (
        count(CheckState::Passed),
        count(CheckState::Failed),
        count(CheckState::Running),
        count(CheckState::Skipped),
    )
}

/// The host and `owner/repo` of a pull request's URL: `…/pull/12` on GitHub, `…/pulls/12` on
/// Forgejo and Gitea, `…/-/merge_requests/12` on GitLab.
pub(crate) fn repo_of_url(url: &str) -> Option<(String, String)> {
    let repo = ["/-/merge_requests/", "/pulls/", "/pull/"]
        .iter()
        .find_map(|marker| url.split_once(marker))?
        .0;
    forge::location(repo)
}

/// The remote of the repository a pull request's URL belongs to.
pub(crate) fn remote_for<'a>(remotes: &'a [Remote], url: &str) -> Option<&'a str> {
    let repo = repo_of_url(url)?;
    remotes
        .iter()
        .find(|remote| forge::location(&remote.url).as_ref() == Some(&repo))
        .map(|remote| remote.name.as_str())
}

/// Whether a link from a pull request may be opened: a click must not run a local file or some
/// other program's address.
pub fn is_web_url(url: &str) -> bool {
    let url = url.trim_start().to_lowercase();
    url.starts_with("https://") || url.starts_with("http://")
}

/// `body` made safe and fit to show: without HTML comments (what templates leave behind), with
/// `<img>` tags as images, and images from anywhere but `host` as links, so that reading a pull
/// request doesn't make the app contact a server its author chose.
pub fn safe_markdown(body: &str, host: &str) -> String {
    let body = without_comments(body);
    let body = img_tags_to_markdown(&body);
    let body = without_html_tags(&body);
    images_to_links(&body, host)
}

/// `body` made safe, and cut into what the page shows one after the other.
pub fn safe_blocks(body: &str, host: &str) -> Vec<Block> {
    // A piece can read differently from the whole, say a table the whole has in a code block, so
    // each is checked for images on its own too.
    let check = |text: &str| images_to_links(text, host);
    split_image_tables(&safe_markdown(body, host))
        .into_iter()
        .map(|block| match block {
            Block::Markdown(text) => Block::Markdown(check(&text)),
            Block::ImageTable { header, rows } => Block::ImageTable {
                header: header.iter().map(|cell| check(cell)).collect(),
                rows: rows
                    .iter()
                    .map(|row| row.iter().map(|cell| check(cell)).collect())
                    .collect(),
            },
        })
        .collect()
}

/// A part of a pull request's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Markdown(String),
    /// A table with images, which the app lays out itself so that they share the width.
    ImageTable {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// `markdown` with its tables of images taken out as blocks of their own. What is in a code block
/// isn't a table, whatever it looks like.
pub fn split_image_tables(markdown: &str) -> Vec<Block> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut blocks = Vec::new();
    let mut text: Vec<&str> = Vec::new();
    let mut fenced = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && is_table_row(line) && lines.get(i + 1).is_some_and(|l| is_delimiter(l)) {
            let end = (i + 2..lines.len())
                .find(|&k| !is_table_row(lines[k]))
                .unwrap_or(lines.len());
            let rows = &lines[i + 2..end];
            if rows.iter().any(|row| row.contains("![")) {
                flush_text(&mut text, &mut blocks);
                blocks.push(Block::ImageTable {
                    header: table_cells(line),
                    rows: rows.iter().map(|row| table_cells(row)).collect(),
                });
                i = end;
                continue;
            }
        }
        text.push(line);
        i += 1;
    }
    flush_text(&mut text, &mut blocks);
    blocks
}

fn flush_text(text: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if text.iter().any(|line| !line.trim().is_empty()) {
        blocks.push(Block::Markdown(text.join("\n")));
    }
    text.clear();
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn is_delimiter(line: &str) -> bool {
    is_table_row(line)
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn table_cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.strip_suffix('|').unwrap_or(row);
    row.split('|').map(|cell| cell.trim().to_owned()).collect()
}

/// `body` without the HTML tags people and bots put in Markdown, which the viewer would show as
/// text. What they enclose stays; code stays as it is.
fn without_html_tags(body: &str) -> String {
    let mut fenced = false;
    let lines: Vec<String> = body
        .split('\n')
        .map(|line| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                return line.to_owned();
            }
            if fenced {
                return line.to_owned();
            }
            // Every other piece between backticks is inline code.
            line.split('`')
                .enumerate()
                .map(|(i, piece)| {
                    if i % 2 == 0 {
                        strip_tags(piece)
                    } else {
                        piece.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("`")
        })
        .collect();
    lines.join("\n")
}

/// The HTML that shows up in pull requests. Anything else in angle brackets is left alone: it may
/// well be `Vec<String>`.
const HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "center",
    "code",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "kbd",
    "li",
    "ol",
    "p",
    "picture",
    "pre",
    "s",
    "small",
    "source",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
];

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match tag_at(after) {
            Some((name, len)) => {
                if name.eq_ignore_ascii_case("br") {
                    out.push('\n');
                }
                rest = &after[len..];
            }
            None => {
                out.push('<');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The name and length of the known tag `text` starts with. A `>` in an attribute's quotes doesn't
/// end the tag.
fn tag_at(text: &str) -> Option<(&str, usize)> {
    let inner = text.strip_prefix('<')?;
    let inner = inner.strip_prefix('/').unwrap_or(inner);
    let len = inner
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(inner.len());
    let name = &inner[..len];
    if !HTML_TAGS.iter().any(|tag| tag.eq_ignore_ascii_case(name)) {
        return None;
    }
    let mut quote = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (None, '>') => return Some((name, i + 1)),
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if open == c => quote = None,
            _ => {}
        }
    }
    None
}

fn without_comments(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn img_tags_to_markdown(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("<img") {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        out.push_str(&rest[..start]);
        let tag = &rest[start..start + len];
        if let Some(src) = attribute(tag, "src") {
            out.push_str(&format!(
                "![{}]({src})",
                attribute(tag, "alt").unwrap_or("")
            ));
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

/// The value of `name="…"` in an HTML tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

/// `body` with its images from anywhere but `host` as links. The viewer's own parser finds them:
/// references, escapes and images in images are all images too, which a search of the text misses.
fn images_to_links(body: &str, host: &str) -> String {
    let mut text = body.to_owned();
    // A link right after a `!` reads as an image again, so go on until nothing changes.
    for _ in 0..MAX_IMAGE_PASSES {
        let next = image_pass(&text, host);
        if next == text {
            return text;
        }
        text = next;
    }
    text.replace("![", "[")
}

const MAX_IMAGE_PASSES: usize = 8;

fn image_pass(body: &str, host: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    let mut events = Parser::new_ext(body, markdown_options()).into_offset_iter();
    while let Some((event, range)) = events.next() {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        if is_trusted_image(&dest_url, host) {
            continue;
        }
        let mut label = String::new();
        let mut depth = 1;
        for (inner, _) in events.by_ref() {
            match inner {
                Event::Start(Tag::Image { .. }) => depth += 1,
                Event::End(TagEnd::Image) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Event::Text(text) | Event::Code(text) => label.push_str(&text),
                _ => {}
            }
        }
        let Some(before) = body.get(copied..range.start) else {
            continue;
        };
        out.push_str(before);
        out.push_str(&link_markdown(&label, &dest_url));
        copied = range.end;
    }
    out.push_str(&body[copied..]);
    out
}

fn link_markdown(label: &str, url: &str) -> String {
    let label = if label.trim().is_empty() {
        "image".to_owned()
    } else {
        label
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace(['\n', '\r'], " ")
    };
    let plain = !url
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>' | '\\'));
    if plain {
        format!("[{label}]({url})")
    } else {
        let url = url
            .replace('<', "%3C")
            .replace('>', "%3E")
            .replace('\\', "%5C")
            .replace(['\n', '\r'], "");
        format!("[{label}](<{url}>)")
    }
}

/// What the viewer parses Markdown with, so that this module sees the same images it does.
fn markdown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
}

fn is_trusted_image(url: &str, host: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let image_host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    let plain_name = image_host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    image_host == host
        || (host == "github.com" && plain_name && image_host.ends_with(".githubusercontent.com"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(parse_time("2026-11-02T20:37:34Z"), Some(1_793_651_854));
        assert_eq!(parse_time("2026-11-02T21:37:34+01:00"), Some(1_793_651_854));
        assert_eq!(
            parse_time("2026-11-02T15:37:34.123-05:00"),
            Some(1_793_651_854)
        );
        assert_eq!(parse_time("2026-11-02T20:37:34"), None);
        assert_eq!(parse_time("not a time"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn finds_the_repository_of_a_pull_request() {
        assert_eq!(
            repo_of_url("https://github.com/o/r/pull/12"),
            Some(("github.com".to_owned(), "o/r".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://codeberg.org/o/r/pulls/12"),
            Some(("codeberg.org".to_owned(), "o/r".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://gitlab.com/g/sub/r/-/merge_requests/3"),
            Some(("gitlab.com".to_owned(), "g/sub/r".to_owned()))
        );
        assert_eq!(repo_of_url("https://github.com/o/r"), None);
    }

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.to_owned(),
            url: url.to_owned(),
        }
    }

    #[test]
    fn finds_the_remote_of_a_pull_request_whatever_its_url_looks_like() {
        let remotes = [
            remote("fork", "https://github.com/me/r.git"),
            remote("up", "git@github.com:o/r.git"),
        ];
        assert_eq!(
            remote_for(&remotes, "https://github.com/o/r/pull/1"),
            Some("up")
        );
        assert_eq!(remote_for(&remotes, "https://github.com/x/y/pull/1"), None);
    }

    fn entry(kind: EntryKind) -> Entry {
        Entry {
            kind,
            author: "ana".to_owned(),
            at: 0,
            body: String::new(),
            url: String::new(),
            review_id: None,
        }
    }

    #[test]
    fn a_review_without_a_verdict_still_counts_as_a_review() {
        let reviewed = [entry(EntryKind::Review("reviewed"))];
        assert_eq!(reviews_label("", true, &reviewed), "Reviewed");
        let on_a_line = [entry(EntryKind::Inline {
            path: "a.rs".to_owned(),
            line: Some(1),
            side: DiffSide::Right,
            outdated: false,
            resolved: false,
        })];
        assert_eq!(reviews_label("", true, &on_a_line), "Reviewed");
        let talk = [entry(EntryKind::Comment)];
        assert_eq!(reviews_label("", true, &talk), "No reviews yet");
    }

    #[test]
    fn a_verdict_says_more_than_that_there_was_a_review() {
        let reviewed = [entry(EntryKind::Review("approved"))];
        assert_eq!(reviews_label("APPROVED", true, &reviewed), "Approved");
    }

    #[test]
    fn no_review_is_coming_on_a_finished_pull_request() {
        assert_eq!(reviews_label("", false, &[]), "No reviews");
    }

    #[test]
    fn images_from_the_pull_requests_host_stay() {
        let body = "![shot](https://github.com/user-attachments/assets/a)";
        assert_eq!(safe_markdown(body, "github.com"), body);
        let raw = "![shot](https://raw.githubusercontent.com/me/r/1/a.png)";
        assert_eq!(safe_markdown(raw, "github.com"), raw);
    }

    #[test]
    fn images_from_elsewhere_become_links() {
        assert_eq!(
            safe_markdown("a ![pixel](https://evil.example/x.png) b", "github.com"),
            "a [pixel](https://evil.example/x.png) b"
        );
        // Only the host counts, not what the address merely contains.
        assert_eq!(
            safe_markdown("![x](https://github.com.evil.example/x.png)", "github.com"),
            "[x](https://github.com.evil.example/x.png)"
        );
        assert_eq!(
            safe_markdown("![x](http://github.com/x.png)", "github.com"),
            "[x](http://github.com/x.png)"
        );
    }

    /// Where the viewer would fetch images from, as its own parser sees the text.
    fn image_sources(markdown: &str) -> Vec<String> {
        pulldown_cmark::Parser::new_ext(markdown, markdown_options())
            .filter_map(|event| match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                    Some(dest_url.to_string())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_way_of_writing_an_image_gets_one_fetched_from_elsewhere() {
        let evil = "https://evil.example/p.png";
        for body in [
            format!("![x][a]\n\n[a]: {evil}"),
            format!("![a][]\n\n[a]: {evil}"),
            format!("![a]\n\n[a]: {evil}"),
            format!("![a\\]b]({evil})"),
            format!("!![x]({evil})"),
            format!("!!!![x]({evil})"),
            format!("![![x]({evil})](https://github.com/a.png)"),
            format!("![![x](https://github.com/a.png)]({evil})"),
            format!("![x](<{evil}>)"),
            format!("![x]({evil} \"title\")"),
            format!("> ![x]({evil})"),
            format!("- ![x]({evil})"),
        ] {
            let shown = safe_markdown(&body, "github.com");
            assert!(
                image_sources(&shown)
                    .iter()
                    .all(|url| !url.contains("evil")),
                "{body:?} became {shown:?}"
            );
            assert!(shown.contains("evil.example"), "the link went: {shown:?}");
        }
    }

    #[test]
    fn a_table_outside_the_parsers_view_gets_no_image_through() {
        for body in [
            "~~~\n| h |\n|---|\n| ![x](https://evil.example/p.png) |\n~~~",
            "    | h |\n    |---|\n    | ![x](https://evil.example/p.png) |",
        ] {
            for block in safe_blocks(body, "github.com") {
                match block {
                    Block::Markdown(text) => {
                        assert!(image_sources(&text).is_empty(), "{body:?}: {text:?}");
                    }
                    Block::ImageTable { header, rows } => {
                        for cell in header.iter().chain(rows.iter().flatten()) {
                            assert!(image_sources(cell).is_empty(), "{body:?}: {cell:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_host_that_only_ends_right_is_no_trusted_host() {
        for url in [
            r"https://evil.example\.githubusercontent.com/p.png",
            "https://evil.example%2f.githubusercontent.com/p.png",
            "https://evil.example\t.githubusercontent.com/p.png",
            "https://user@evil.example/.githubusercontent.com",
        ] {
            assert!(!is_trusted_image(url, "github.com"), "{url}");
        }
        assert!(is_trusted_image(
            "https://user-images.githubusercontent.com/p.png",
            "github.com"
        ));
    }

    #[test]
    fn what_looks_like_an_image_in_code_stays_code() {
        let block = "```\n![x](https://evil.example/p.png)\n```";
        assert_eq!(safe_markdown(block, "github.com"), block);
        let inline = "Write `![x](https://evil.example/p.png)` for it";
        assert_eq!(safe_markdown(inline, "github.com"), inline);
    }

    #[test]
    fn an_image_link_without_text_still_has_something_to_click() {
        assert_eq!(
            safe_markdown("![](https://evil.example/x.png)", "github.com"),
            "[image](https://evil.example/x.png)"
        );
    }

    #[test]
    fn img_tags_become_images_by_the_same_rule() {
        assert_eq!(
            safe_markdown(
                r#"<img width="400" alt="Shot" src="https://github.com/user-attachments/assets/a" />"#,
                "github.com"
            ),
            "![Shot](https://github.com/user-attachments/assets/a)"
        );
        assert_eq!(
            safe_markdown(r#"<img src="https://evil.example/x.png">"#, "github.com"),
            "[image](https://evil.example/x.png)"
        );
    }

    #[test]
    fn html_comments_are_left_out() {
        assert_eq!(
            safe_markdown("a<!-- template\nhint -->b", "github.com"),
            "ab"
        );
    }

    #[test]
    fn plain_text_with_exclamation_marks_is_left_alone() {
        let body = "Wow![not an image] and [link](https://a.example) ![unfinished](";
        assert_eq!(safe_markdown(body, "github.com"), body);
    }

    #[test]
    fn only_web_links_may_be_opened() {
        assert!(is_web_url("https://github.com/o/r"));
        assert!(is_web_url("HTTP://example.com"));
        assert!(!is_web_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("ms-settings:"));
        assert!(!is_web_url(""));
    }

    fn text(markdown: &str) -> Block {
        Block::Markdown(markdown.to_owned())
    }

    #[test]
    fn text_without_tables_stays_one_block() {
        assert_eq!(split_image_tables("a\n\nb"), [text("a\n\nb")]);
    }

    #[test]
    fn no_text_makes_no_blocks() {
        assert_eq!(split_image_tables(""), []);
    }

    #[test]
    fn a_table_of_images_becomes_a_block_between_the_text_around_it() {
        let markdown = "before\n\n| A | B |\n|---|:--:|\n| ![a](u) | ![b](v) |\n\nafter";
        assert_eq!(
            split_image_tables(markdown),
            [
                text("before\n"),
                Block::ImageTable {
                    header: vec!["A".to_owned(), "B".to_owned()],
                    rows: vec![vec!["![a](u)".to_owned(), "![b](v)".to_owned()]],
                },
                text("\nafter"),
            ]
        );
    }

    #[test]
    fn a_table_without_images_is_left_to_the_markdown() {
        let markdown = "| A |\n|---|\n| text |";
        assert_eq!(split_image_tables(markdown), [text(markdown)]);
    }

    #[test]
    fn a_table_at_the_very_start_and_end_has_no_empty_text_around_it() {
        let blocks = split_image_tables("| A |\n|---|\n| ![a](u) |");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::ImageTable { .. }));
    }

    #[test]
    fn windows_line_ends_make_no_difference() {
        let blocks = split_image_tables("| A |\r\n|---|\r\n| ![a](u) |\r\n");
        assert!(matches!(blocks[0], Block::ImageTable { .. }));
    }

    #[test]
    fn a_table_in_a_code_block_is_code() {
        let markdown = "```\n| A |\n|---|\n| ![a](u) |\n```";
        assert_eq!(split_image_tables(markdown), [text(markdown)]);
    }

    #[test]
    fn pipes_without_a_delimiter_row_are_not_a_table() {
        let markdown = "| ![a](u) |\n| ![b](v) |";
        assert_eq!(split_image_tables(markdown), [text(markdown)]);
    }

    #[test]
    fn html_tags_go_and_what_they_enclose_stays() {
        let shown = safe_markdown(
            "<details>\n<summary><strong>Title</strong></summary>\n\nInner text\n</details>",
            "github.com",
        );
        assert!(!shown.contains('<'), "{shown}");
        assert!(shown.contains("Title"), "{shown}");
        assert!(shown.contains("Inner text"), "{shown}");
    }

    #[test]
    fn tags_with_attributes_go_too() {
        assert_eq!(
            safe_markdown(r#"<strong title="a > b">Auto</strong> fix"#, "github.com"),
            "Auto fix"
        );
        assert_eq!(
            safe_markdown(
                r#"<picture><source media="x" srcset="u"></picture>"#,
                "github.com"
            ),
            ""
        );
    }

    #[test]
    fn a_line_break_tag_breaks_the_line() {
        assert_eq!(safe_markdown("a<br/>b<br>c", "github.com"), "a\nb\nc");
    }

    #[test]
    fn what_only_looks_like_a_tag_stays() {
        for text in ["a < b and c > d", "Vec<String> is <unknown>", "x <- y"] {
            assert_eq!(safe_markdown(text, "github.com"), text);
        }
    }

    #[test]
    fn tags_in_code_stay() {
        let inline = "Use `<details>` here";
        assert_eq!(safe_markdown(inline, "github.com"), inline);
        let block = "```html\n<details>x</details>\n```";
        assert_eq!(safe_markdown(block, "github.com"), block);
    }

    #[test]
    fn enterprise_hosts_trust_themselves() {
        let body = "![a](https://git.corp.example/x.png)";
        assert_eq!(safe_markdown(body, "git.corp.example"), body);
    }
}
