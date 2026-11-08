//! gitcraft's command line (`gitcraft log <file>`, `gitcraft blame <file>:<line>`…): reading
//! it ([`args`]) and making sense of it on disk ([`resolve`]).

pub mod args;
mod display;
pub use display::message as display_message;
pub mod link;
pub mod open_link;
pub mod resolve;

pub use args::{Command, Parsed, parse};
pub use resolve::{Resolved, Then};

use crate::repo::Action;

/// What the repository's tab does for `then`, once it has read the repository.
pub(crate) fn actions(then: Then) -> Vec<Action> {
    use crate::repo::file_view::FileTab;
    match then {
        Then::Nothing => Vec::new(),
        Then::History(path) => vec![Action::FileView {
            path,
            rev: None,
            tab: FileTab::History,
        }],
        Then::Blame { path, line: None } => vec![Action::FileView {
            path,
            rev: None,
            tab: FileTab::Blame,
        }],
        Then::Blame {
            path,
            line: Some(line),
        } => vec![Action::BlameAt {
            path,
            rev: None,
            line: line as usize,
        }],
        Then::FileAt {
            rev,
            path,
            line: None,
        } => vec![Action::FileView {
            path,
            rev,
            tab: FileTab::Blame,
        }],
        Then::FileAt {
            rev,
            path,
            line: Some(line),
        } => vec![Action::BlameAt {
            path,
            rev,
            line: line as usize,
        }],
        Then::Commit(hash) => vec![Action::SelectHash(hash)],
        Then::PullRequest(number) => vec![Action::OpenPr(number)],
        // Cloned first, by the window (see `app::launch`).
        Then::Clone(_) => Vec::new(),
        Then::Compare {
            base,
            base_label,
            target,
            target_label,
            since_fork,
        } => vec![Action::Compare {
            base,
            base_label,
            target: Some(target),
            target_label,
            captions: [None, None],
            since_fork,
        }],
    }
}
