//! Saying something on a pull request, whichever forge it's on: comments and reviews with their
//! comments on lines, replies, suggested changes, merging. [`Target`] is what each forge's module
//! implements; the rest is what they share.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

use crate::pull_request::DiffSide;

/// What a review says about the pull request as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Comment,
    Approve,
    RequestChanges,
}

/// A comment on a line, written and not sent yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineComment {
    pub path: String,
    pub line: u64,
    pub side: DiffSide,
    pub body: String,
}

/// Why this review can't be sent, if it can't. The forge would refuse the same, less clearly.
pub fn review_problem(verdict: Verdict, body: &str, pending: usize) -> Option<&'static str> {
    let says_something = !body.trim().is_empty() || pending > 0;
    match verdict {
        Verdict::Approve => None,
        _ if says_something => None,
        Verdict::Comment => Some("Write something to send"),
        Verdict::RequestChanges => Some("Say what should change"),
    }
}

/// Whether `viewer` is the author of a pull request: forges let nobody review their own.
pub fn is_own(author: &str, viewer: Option<&str>) -> bool {
    viewer.is_some_and(|viewer| viewer.eq_ignore_ascii_case(author))
}

/// How a pull request gets merged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

/// Why this pull request can't be merged, if it can't.
pub fn merge_blocker(
    draft: bool,
    mergeable: &str,
    merge_state: &str,
    can_write: bool,
    can_bypass_rules: bool,
) -> Option<&'static str> {
    if !can_write {
        Some("You don't have the rights to merge here.")
    } else if draft {
        Some("It's still a draft.")
    } else if mergeable == "CONFLICTING" {
        Some("It has conflicts to resolve first.")
    } else if mergeable != "MERGEABLE" {
        Some("GitHub is still checking whether it can be merged. Load it again in a moment.")
    } else if merge_state == "BLOCKED" && !can_bypass_rules {
        Some("The repository's rules don't allow it yet: required reviews or checks are missing.")
    } else {
        None
    }
}

/// The suggestion to offer, only if what would be committed is what the comment shows. Text can
/// hide in a comment's source, for one in an HTML comment.
pub fn shown_suggestion(body: &str, host: &str) -> Option<String> {
    let raw = parse_suggestion(body)?;
    let shown = parse_suggestion(&crate::pull_request::safe_markdown(body, host))?;
    (raw == shown).then_some(raw)
}

/// The first suggested change in a comment's text, each line ended, so that no text at all (remove
/// the line) differs from one blank line.
pub fn parse_suggestion(body: &str) -> Option<String> {
    let mut lines = body.lines();
    lines.find(|line| line.trim_end() == "```suggestion")?;
    let mut suggestion = String::new();
    for line in lines {
        if line.trim_end() == "```" {
            return Some(suggestion);
        }
        suggestion.push_str(line);
        suggestion.push('\n');
    }
    None
}

/// `content` with lines `start` to `end` (counted from 1) replaced by `replacement`. Line ends stay
/// what the file uses, and a last line without one stays without.
pub(crate) fn apply_lines(
    content: &str,
    start: u64,
    end: u64,
    replacement: &str,
) -> Result<String, String> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    if start == 0 || start > end || end as usize > lines.len() {
        return Err(format!(
            "lines {start} to {end} aren't in the file, which has {}",
            lines.len()
        ));
    }
    let line_end = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (from, to) = (start as usize - 1, end as usize);
    let keeps_no_end = to == lines.len() && !lines[to - 1].ends_with('\n');
    let mut out = lines[..from].concat();
    if !replacement.is_empty() {
        out.push_str(&replacement.lines().collect::<Vec<_>>().join(line_end));
        if !keeps_no_end {
            out.push_str(line_end);
        }
    }
    out.push_str(&lines[to..].concat());
    Ok(out)
}

/// A file's path for a URL: every part encoded, the slashes between them not.
pub(crate) fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'/' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ if byte.is_ascii_alphanumeric() => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The text of a file as a contents API (GitHub's, Forgejo's, Gitea's) sends it; `forge` names
/// who sent it.
pub(crate) fn decode_file(file: &Value, forge: &str) -> Result<String, String> {
    // Above 1 MB GitHub's answer has no content, whatever the file holds.
    if file.get("encoding").and_then(Value::as_str) == Some("none") {
        return Err(format!("{forge} didn't send the file: it's too large"));
    }
    let encoded: String = file
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{forge} didn't send the file: it may be too large"))?
        .split_whitespace()
        .collect();
    let bytes = BASE64
        .decode(encoded)
        .map_err(|e| format!("unexpected file from {forge}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| "It isn't a text file".to_owned())
}

/// A suggested change to apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub path: String,
    pub start: u64,
    pub end: u64,
    pub text: String,
    pub author: String,
}

/// A pull request to say something on, on its forge. Each method sends one thing and answers
/// whether it went through; what the forge said when it didn't.
pub trait Target: Send {
    /// `owner/name` of the repository.
    fn path(&self) -> &str;

    /// Adds a comment to the conversation.
    fn comment(&self, body: &str) -> Result<(), String>;

    /// Sends the review: the pending one if there is one, with the comments in it.
    fn review(&self, verdict: Verdict, body: &str) -> Result<(), String>;

    /// Sends a comment on a line right away, as a review of its own.
    fn comment_now(&self, comment: &LineComment) -> Result<(), String>;

    /// Adds a comment on a line to the pending review, starting one if there is none.
    fn add_pending(&self, comment: &LineComment) -> Result<(), String>;

    /// Takes a comment out of the pending review; the review goes with its `last` comment.
    fn remove_pending(&self, comment: &str, last: bool) -> Result<(), String>;

    /// Throws the pending review away.
    fn discard_pending(&self) -> Result<(), String>;

    /// Answers in a thread (into the pending review, when there is one).
    fn reply(&self, thread: &str, body: &str) -> Result<(), String>;

    fn set_resolved(&self, thread: &str, resolved: bool) -> Result<(), String>;

    fn dismiss(&self, review: &str, message: &str) -> Result<(), String>;

    /// Merges the pull request, and deletes its branch if asked to. Only what the page showed is
    /// merged: commits pushed since make it fail.
    fn merge(&self, method: MergeMethod, delete_branch: bool) -> Result<(), String>;

    /// Commits a suggested change to the pull request's branch, onto the file as it is in the
    /// commit that was reviewed: if it has changed since, the forge refuses instead of the change
    /// landing on the wrong lines.
    fn apply_suggestion(&self, suggestion: &Suggestion) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_approval_needs_no_words() {
        assert_eq!(review_problem(Verdict::Approve, "", 0), None);
    }

    #[test]
    fn requesting_changes_has_to_say_which() {
        assert!(review_problem(Verdict::RequestChanges, "  \n", 0).is_some());
        assert_eq!(review_problem(Verdict::RequestChanges, "Fix it", 0), None);
    }

    #[test]
    fn requesting_changes_can_say_so_on_the_lines_alone() {
        assert_eq!(review_problem(Verdict::RequestChanges, "", 1), None);
    }

    #[test]
    fn a_comment_review_has_to_say_something_somewhere() {
        assert!(review_problem(Verdict::Comment, "", 0).is_some());
        assert_eq!(review_problem(Verdict::Comment, "Nice", 0), None);
        assert_eq!(review_problem(Verdict::Comment, "", 2), None);
    }

    #[test]
    fn a_ready_pull_request_can_be_merged() {
        assert_eq!(
            merge_blocker(false, "MERGEABLE", "CLEAN", true, false),
            None
        );
        // Failing checks that aren't required don't stop a merge on GitHub either.
        assert_eq!(
            merge_blocker(false, "MERGEABLE", "UNSTABLE", true, false),
            None
        );
        assert_eq!(
            merge_blocker(false, "MERGEABLE", "BEHIND", true, false),
            None
        );
    }

    #[test]
    fn what_keeps_a_pull_request_from_being_merged() {
        assert!(merge_blocker(true, "MERGEABLE", "DRAFT", true, false).is_some());
        assert!(merge_blocker(false, "CONFLICTING", "DIRTY", true, false).is_some());
        assert!(merge_blocker(false, "UNKNOWN", "UNKNOWN", true, false).is_some());
        assert!(merge_blocker(false, "MERGEABLE", "BLOCKED", true, false).is_some());
        assert!(merge_blocker(false, "MERGEABLE", "CLEAN", false, false).is_some());
    }

    #[test]
    fn rules_are_no_blocker_for_who_may_bypass_them() {
        assert_eq!(
            merge_blocker(false, "MERGEABLE", "BLOCKED", true, true),
            None
        );
        assert!(merge_blocker(false, "CONFLICTING", "DIRTY", true, true).is_some());
        assert!(merge_blocker(true, "MERGEABLE", "DRAFT", true, true).is_some());
    }

    #[test]
    fn a_file_too_large_for_the_api_is_said_to_be() {
        let large = json!({"content": "", "encoding": "none"});
        assert!(decode_file(&large, "GitHub").is_err_and(|e| e.contains("too large")));
        let small = json!({"content": "aGk=\n", "encoding": "base64"});
        assert_eq!(decode_file(&small, "Forgejo").as_deref(), Ok("hi"));
    }

    #[test]
    fn finds_a_suggested_change() {
        let body = "Clearer:\n\n```suggestion\nlet a = 1;\nlet b = 2;\n```\nThanks";
        assert_eq!(
            parse_suggestion(body).as_deref(),
            Some("let a = 1;\nlet b = 2;\n")
        );
    }

    #[test]
    fn a_blank_line_is_not_the_same_as_no_line() {
        let blank = parse_suggestion("```suggestion\n\n```").unwrap_or_default();
        assert_eq!(
            apply_lines("a\nb\nc\n", 2, 2, &blank).as_deref(),
            Ok("a\n\nc\n")
        );
        let gone = parse_suggestion("```suggestion\n```").unwrap_or_default();
        assert_eq!(
            apply_lines("a\nb\nc\n", 2, 2, &gone).as_deref(),
            Ok("a\nc\n")
        );
        let ends_blank = parse_suggestion("```suggestion\nB\n\n```").unwrap_or_default();
        assert_eq!(
            apply_lines("a\nb\nc\n", 2, 2, &ends_blank).as_deref(),
            Ok("a\nB\n\nc\n")
        );
    }

    #[test]
    fn spaces_at_the_end_of_a_suggested_line_stay() {
        assert_eq!(
            parse_suggestion("```suggestion\nx  \n```  ").as_deref(),
            Some("x  \n")
        );
    }

    #[test]
    fn a_suggestion_hidden_from_the_reader_is_not_offered() {
        let hidden = "Looks fine\n<!--\n```suggestion\nevil();\n```\n-->";
        assert_eq!(shown_suggestion(hidden, "github.com"), None);
        let decoy = "<!--\n```suggestion\nevil();\n```\n-->\n```suggestion\ngood();\n```";
        assert_eq!(shown_suggestion(decoy, "github.com"), None);
        let inside = "```suggestion\ngood(); <!-- evil(); -->\n```";
        assert_eq!(shown_suggestion(inside, "github.com"), None);
    }

    #[test]
    fn a_suggestion_in_plain_view_is_offered() {
        let body = "Clearer:\n\n```suggestion\nlet a = 1;\n```";
        assert_eq!(
            shown_suggestion(body, "github.com").as_deref(),
            Some("let a = 1;\n")
        );
    }

    #[test]
    fn a_suggestion_can_be_to_remove_the_line() {
        assert_eq!(parse_suggestion("```suggestion\n```").as_deref(), Some(""));
    }

    #[test]
    fn other_code_blocks_are_no_suggestion() {
        assert_eq!(parse_suggestion("```rust\nlet a = 1;\n```"), None);
        assert_eq!(parse_suggestion("no code"), None);
        assert_eq!(parse_suggestion("```suggestion\nnever closed"), None);
    }

    #[test]
    fn windows_line_ends_in_a_comment_make_no_difference() {
        assert_eq!(
            parse_suggestion("```suggestion\r\nx\r\n```\r\n").as_deref(),
            Some("x\n")
        );
    }

    #[test]
    fn replaces_a_line() {
        assert_eq!(
            apply_lines("a\nb\nc\n", 2, 2, "B").as_deref(),
            Ok("a\nB\nc\n")
        );
    }

    #[test]
    fn replaces_several_lines_with_several() {
        assert_eq!(
            apply_lines("a\nb\nc\nd\n", 2, 3, "x\ny\nz").as_deref(),
            Ok("a\nx\ny\nz\nd\n")
        );
    }

    #[test]
    fn an_empty_suggestion_removes_the_line() {
        assert_eq!(apply_lines("a\nb\nc\n", 2, 2, "").as_deref(), Ok("a\nc\n"));
    }

    #[test]
    fn keeps_the_line_ends_of_the_file() {
        assert_eq!(
            apply_lines("a\r\nb\r\nc\r\n", 2, 2, "x\ny").as_deref(),
            Ok("a\r\nx\r\ny\r\nc\r\n")
        );
    }

    #[test]
    fn keeps_a_last_line_without_a_line_end() {
        assert_eq!(apply_lines("a\nb", 2, 2, "B").as_deref(), Ok("a\nB"));
    }

    #[test]
    fn lines_that_are_not_there_are_an_error() {
        assert!(apply_lines("a\n", 3, 3, "x").is_err());
        assert!(apply_lines("a\n", 0, 1, "x").is_err());
        assert!(apply_lines("a\nb\n", 2, 1, "x").is_err());
    }

    #[test]
    fn encodes_a_path_but_not_its_slashes() {
        assert_eq!(encode_path("src/a b#c.rs"), "src/a%20b%23c.rs");
        assert_eq!(encode_path("src/ä.rs"), "src/%C3%A4.rs");
        assert_eq!(encode_path("plain-name_1.2~x"), "plain-name_1.2~x");
    }

    #[test]
    fn nobody_reviews_their_own_pull_request() {
        assert!(is_own("ana", Some("ana")));
        assert!(is_own("Ana", Some("aNA")));
        assert!(!is_own("ana", Some("bo")));
        // Not knowing who is logged in, it's up to the forge to say.
        assert!(!is_own("ana", None));
    }
}
