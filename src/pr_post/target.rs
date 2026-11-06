//! What says or does something on a pull request, implemented by each forge's module, and the
//! helpers they share for it.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

use super::suggestion::Suggestion;
use super::{LineComment, MergeMethod, Verdict};

/// A pull request to say something on, on its forge. Each method sends one thing and answers
/// whether it went through; what the forge said when it didn't.
pub trait Target: Send {
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

/// Whether a pull request's branch may go after its merge, from the commit it's on now (`None`
/// when that couldn't be read): only when it's still the merged one. Commits pushed since would
/// lose their branch, and a tip nobody could read may be one of those.
pub(crate) fn may_delete_branch(tip: Option<&str>, merged: &str) -> Result<(), String> {
    match tip {
        Some(tip) if tip == merged => Ok(()),
        Some(_) => Err("It's merged, but the branch has new commits, so it was kept".to_owned()),
        None => Err(
            "It's merged, but which commit the branch is on couldn't be read, so it was kept"
                .to_owned(),
        ),
    }
}

/// A file's path as a comment gives it, refused if it could climb out of the contents API
/// (`../../other/repo/contents/x`) once the URL is resolved.
pub(crate) fn check_path(path: &str) -> Result<(), String> {
    let climbs = path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if climbs {
        Err(format!("{path:?} isn't a path in the repository"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_file_too_large_for_the_api_is_said_to_be() {
        let large = json!({"content": "", "encoding": "none"});
        assert!(decode_file(&large, "GitHub").is_err_and(|e| e.contains("too large")));
        let small = json!({"content": "aGk=\n", "encoding": "base64"});
        assert_eq!(decode_file(&small, "Forgejo").as_deref(), Ok("hi"));
    }

    #[test]
    fn a_branch_goes_only_while_it_is_on_the_merged_commit() {
        assert_eq!(may_delete_branch(Some("abc"), "abc"), Ok(()));
        assert!(may_delete_branch(Some("def"), "abc").is_err_and(|e| e.contains("new commits")));
        assert!(may_delete_branch(None, "abc").is_err_and(|e| e.contains("couldn't be read")));
    }

    #[test]
    fn a_suggestion_stays_in_its_repository() {
        assert!(check_path("src/a.rs").is_ok());
        for bad in ["../x", "a/../../b", "/etc/passwd", "a//b", "./a", ""] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn encodes_a_path_but_not_its_slashes() {
        assert_eq!(encode_path("src/a b#c.rs"), "src/a%20b%23c.rs");
        assert_eq!(encode_path("src/ä.rs"), "src/%C3%A4.rs");
        assert_eq!(encode_path("plain-name_1.2~x"), "plain-name_1.2~x");
    }
}
