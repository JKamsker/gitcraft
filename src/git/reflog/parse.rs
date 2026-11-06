//! What a reflog message says moved the ref: `checkout: moving from main to topic`,
//! `commit (amend): Fix the parser`, `reset: moving to HEAD~2`…

/// What moved a ref, as its reflog message names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflogKind {
    /// `commit (initial)`: the first commit of the branch.
    Initial,
    Commit,
    Amend,
    /// `merge x`, `commit (merge)`.
    Merge,
    Checkout,
    Reset,
    /// Every step of a rebase: `rebase (start)`, `rebase -i (pick)`, `rebase (finish)`…
    Rebase,
    /// `pull`, `pull --rebase (finish)`…
    Pull,
    CherryPick,
    Revert,
    /// `branch: Created from HEAD`, `Branch: renamed …`.
    Branch,
    Clone,
    Fetch,
    Push,
    /// `am`: a patch applied with `git am`.
    Am,
    /// Anything else (another tool's message, or none at all).
    Other,
}

impl ReflogKind {
    /// How the view names it.
    pub fn label(self) -> &'static str {
        match self {
            ReflogKind::Initial => "initial commit",
            ReflogKind::Commit => "commit",
            ReflogKind::Amend => "amend",
            ReflogKind::Merge => "merge",
            ReflogKind::Checkout => "checkout",
            ReflogKind::Reset => "reset",
            ReflogKind::Rebase => "rebase",
            ReflogKind::Pull => "pull",
            ReflogKind::CherryPick => "cherry-pick",
            ReflogKind::Revert => "revert",
            ReflogKind::Branch => "branch",
            ReflogKind::Clone => "clone",
            ReflogKind::Fetch => "fetch",
            ReflogKind::Push => "push",
            ReflogKind::Am => "am",
            ReflogKind::Other => "other",
        }
    }
}

/// A reflog message taken apart: what moved the ref, the command as it wrote itself
/// (`rebase -i (pick)`), and what it said after the colon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflogMessage {
    pub kind: ReflogKind,
    /// The part before the first `: ` (the whole message when there is none).
    pub action: String,
    /// The part after it: a commit's summary, `moving from main to topic`…
    pub detail: String,
}

/// Takes a reflog message apart. git writes `<command>: <what it did>`; a message without a
/// colon (`update by push`, another tool's own) is all action.
pub fn parse_message(message: &str) -> ReflogMessage {
    let message = message.trim();
    let (action, detail) = match message.split_once(": ") {
        Some((action, detail)) => (action.trim(), detail.trim()),
        None => (message.strip_suffix(':').unwrap_or(message).trim(), ""),
    };
    ReflogMessage {
        kind: kind_of(action),
        action: action.to_owned(),
        detail: detail.to_owned(),
    }
}

/// The kind an action (`commit (amend)`, `pull --rebase (finish)`, `merge origin/main`) is.
fn kind_of(action: &str) -> ReflogKind {
    let lower = action.to_lowercase();
    let word = lower
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or_default();
    // `commit (amend)`, `commit (merge)`, `commit (initial)`: the note in brackets says more.
    let note = lower
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(note, _)| note.trim());
    match (word, note) {
        ("commit", Some("initial")) => ReflogKind::Initial,
        ("commit", Some("amend")) => ReflogKind::Amend,
        ("commit", Some("merge")) => ReflogKind::Merge,
        ("commit", Some("cherry-pick")) => ReflogKind::CherryPick,
        ("commit", _) => ReflogKind::Commit,
        ("merge", _) => ReflogKind::Merge,
        ("checkout" | "switch", _) => ReflogKind::Checkout,
        ("reset", _) => ReflogKind::Reset,
        ("rebase", _) => ReflogKind::Rebase,
        ("pull", _) => ReflogKind::Pull,
        ("cherry-pick", _) => ReflogKind::CherryPick,
        ("revert", _) => ReflogKind::Revert,
        ("branch", _) => ReflogKind::Branch,
        ("clone", _) => ReflogKind::Clone,
        ("fetch", _) => ReflogKind::Fetch,
        ("push", _) => ReflogKind::Push,
        ("update", _) if lower.starts_with("update by push") => ReflogKind::Push,
        ("am", _) => ReflogKind::Am,
        _ => ReflogKind::Other,
    }
}

/// The time and date in a reflog selector read with `--date=format-local:%s %Y-%m-%d %H:%M`
/// (`main@{1700000000 2023-11-14 22:13}`): the Unix time and the local date.
pub(super) fn selector_time(selector: &str) -> Option<(i64, String)> {
    let (_, inside) = selector.rsplit_once("@{")?;
    let inside = inside.strip_suffix('}')?;
    let (secs, date) = inside.split_once(' ').unwrap_or((inside, ""));
    Some((secs.trim().parse().ok()?, date.trim().to_owned()))
}

/// The number in a reflog selector (`HEAD@{3}` → 3).
pub(super) fn selector_index(selector: &str) -> Option<usize> {
    let (_, inside) = selector.rsplit_once("@{")?;
    inside.strip_suffix('}')?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(message: &str) -> ReflogKind {
        parse_message(message).kind
    }

    /// The messages git itself writes, each told apart.
    #[test]
    fn every_common_message_has_its_kind() {
        let cases = [
            ("commit (initial): First", ReflogKind::Initial),
            ("commit: Add the parser", ReflogKind::Commit),
            ("commit (amend): Add the parser", ReflogKind::Amend),
            ("commit (merge): Merge branch 'x'", ReflogKind::Merge),
            ("commit (cherry-pick): Fix", ReflogKind::CherryPick),
            ("merge topic: Fast-forward", ReflogKind::Merge),
            (
                "merge origin/main: Merge made by the 'ort' strategy.",
                ReflogKind::Merge,
            ),
            ("checkout: moving from main to topic", ReflogKind::Checkout),
            ("reset: moving to HEAD~1", ReflogKind::Reset),
            ("rebase (start): checkout main", ReflogKind::Rebase),
            ("rebase (pick): Add the parser", ReflogKind::Rebase),
            ("rebase -i (squash): Fix", ReflogKind::Rebase),
            (
                "rebase (finish): returning to refs/heads/topic",
                ReflogKind::Rebase,
            ),
            (
                "rebase -i (finish): refs/heads/topic onto 1234",
                ReflogKind::Rebase,
            ),
            ("pull: Fast-forward", ReflogKind::Pull),
            (
                "pull --rebase (finish): returning to refs/heads/main",
                ReflogKind::Pull,
            ),
            ("pull origin main: Fast-forward", ReflogKind::Pull),
            ("cherry-pick: Fix the parser", ReflogKind::CherryPick),
            ("revert: Revert \"Fix\"", ReflogKind::Revert),
            ("branch: Created from HEAD", ReflogKind::Branch),
            (
                "Branch: renamed refs/heads/a to refs/heads/b",
                ReflogKind::Branch,
            ),
            ("clone: from https://example.com/r.git", ReflogKind::Clone),
            ("fetch origin main:main: fast-forward", ReflogKind::Fetch),
            ("update by push", ReflogKind::Push),
            ("push", ReflogKind::Push),
            ("am: Fix the parser", ReflogKind::Am),
        ];
        for (message, want) in cases {
            assert_eq!(kind(message), want, "{message}");
        }
    }

    #[test]
    fn the_action_and_its_detail_split_at_the_first_colon() {
        let m = parse_message("commit: Fix: the parser: again");
        assert_eq!(m.action, "commit");
        assert_eq!(m.detail, "Fix: the parser: again");
        let m = parse_message("rebase -i (pick): Fix");
        assert_eq!(m.action, "rebase -i (pick)");
        assert_eq!(m.detail, "Fix");
    }

    /// No colon, nothing at all, a tool of its own, a colon without a space.
    #[test]
    fn unusual_messages_are_kept_whole() {
        let m = parse_message("update by push");
        assert_eq!(
            (m.action.as_str(), m.detail.as_str()),
            ("update by push", "")
        );
        let m = parse_message("");
        assert_eq!(m.kind, ReflogKind::Other);
        assert_eq!((m.action.as_str(), m.detail.as_str()), ("", ""));
        let m = parse_message("filter-branch: rewrite");
        assert_eq!(m.kind, ReflogKind::Other);
        assert_eq!(m.action, "filter-branch");
        let m = parse_message("my-tool:moved it");
        assert_eq!(m.kind, ReflogKind::Other);
        assert_eq!(m.action, "my-tool:moved it");
        let m = parse_message("  reset:  ");
        assert_eq!(m.kind, ReflogKind::Reset);
        assert_eq!(m.detail, "");
        // A bracket that never closes, a word in capitals.
        assert_eq!(kind("commit (amend: x"), ReflogKind::Commit);
        assert_eq!(kind("CHECKOUT: moving from a to b"), ReflogKind::Checkout);
    }

    #[test]
    fn unicode_stays_as_written() {
        let m = parse_message("commit: Ünïcödé 修正 🚀");
        assert_eq!(m.kind, ReflogKind::Commit);
        assert_eq!(m.detail, "Ünïcödé 修正 🚀");
        let m = parse_message("checkout: moving from main to fünf/日本");
        assert_eq!(m.kind, ReflogKind::Checkout);
        assert_eq!(m.detail, "moving from main to fünf/日本");
        // A non-ASCII action, whose lowercase is longer than it.
        assert_eq!(kind("İmport: x"), ReflogKind::Other);
    }

    #[test]
    fn selectors_give_their_time_and_number() {
        assert_eq!(
            selector_time("HEAD@{1700000000 2023-11-14 22:13}"),
            Some((1_700_000_000, "2023-11-14 22:13".to_owned()))
        );
        assert_eq!(
            selector_time("fünf/日本@{1700000000 2023-11-14 22:13}"),
            Some((1_700_000_000, "2023-11-14 22:13".to_owned()))
        );
        assert_eq!(
            selector_time("HEAD@{1700000000}"),
            Some((1_700_000_000, String::new()))
        );
        assert_eq!(selector_time("HEAD"), None);
        assert_eq!(selector_time("HEAD@{x 2023}"), None);
        assert_eq!(selector_index("HEAD@{0}"), Some(0));
        assert_eq!(selector_index("refs/heads/a@b@{12}"), Some(12));
        assert_eq!(selector_index("HEAD@{12"), None);
        assert_eq!(selector_index("HEAD@{now}"), None);
    }
}
