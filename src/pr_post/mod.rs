//! Saying something on a pull request, whichever forge it's on: comments and reviews with their
//! comments on lines, replies, suggested changes, merging. [`Target`] is what each forge's module
//! implements; the rest is what they share.

mod suggestion;
mod target;

use crate::pull_request::DiffSide;

pub(crate) use suggestion::apply_lines;
pub use suggestion::{Suggestion, shown_suggestion};
pub use target::Target;
pub(crate) use target::{check_path, decode_file, encode_path, may_delete_branch};

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn nobody_reviews_their_own_pull_request() {
        assert!(is_own("ana", Some("ana")));
        assert!(is_own("Ana", Some("aNA")));
        assert!(!is_own("ana", Some("bo")));
        // Not knowing who is logged in, it's up to the forge to say.
        assert!(!is_own("ana", None));
    }
}
