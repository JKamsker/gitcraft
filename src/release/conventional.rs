//! Commit messages read as Conventional Commits (`feat(ui)!: summary`, `BREAKING CHANGE:` in
//! the body), and the pull requests they name.

use super::version::Bump;

/// One commit, as the release notes see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub hash: String,
    /// `feat`, `fix`… lowercase; empty for a message that isn't a conventional one.
    pub kind: String,
    pub scope: String,
    pub breaking: bool,
    /// The summary without its type.
    pub summary: String,
    /// The pull requests it names (`(#12)`, `Merge pull request #12`).
    pub prs: Vec<u64>,
}

impl Change {
    /// Reads the commit `hash` with message `subject` and `body`.
    pub(crate) fn parse(hash: &str, subject: &str, body: &str) -> Change {
        let subject = subject.trim();
        let mut change = Change {
            hash: hash.to_owned(),
            kind: String::new(),
            scope: String::new(),
            breaking: false,
            summary: subject.to_owned(),
            prs: pull_requests(subject),
        };
        if let Some((head, summary)) = subject.split_once(": ") {
            let bang = head.ends_with('!');
            let head = head.trim_end_matches('!');
            let (kind, scope) = match head.split_once('(') {
                Some((kind, scope)) if scope.ends_with(')') => (kind, scope.trim_end_matches(')')),
                Some(_) => ("", ""),
                None => (head, ""),
            };
            if !kind.is_empty() && kind.chars().all(|c| c.is_ascii_alphanumeric()) {
                change.kind = kind.to_lowercase();
                change.scope = scope.to_owned();
                change.breaking = bang;
                change.summary = summary.trim().to_owned();
            }
        }
        if body.lines().any(|l| {
            let l = l.trim_start();
            l.starts_with("BREAKING CHANGE:") || l.starts_with("BREAKING-CHANGE:")
        }) {
            change.breaking = true;
        }
        change
    }

    /// How much this change moves the version.
    pub(crate) fn bump(&self) -> Bump {
        if self.breaking {
            Bump::Major
        } else if self.kind == "feat" {
            Bump::Minor
        } else {
            Bump::Patch
        }
    }

    /// A commit that made an earlier release (`chore(release): v1.2.3`), left out of the notes.
    pub(crate) fn is_release(&self) -> bool {
        self.kind == "chore" && self.scope == "release"
    }
}

/// The numbers of the pull requests `text` names: `#12` after `(`, a space or at the start, and
/// GitLab's `!12`.
fn pull_requests(text: &str) -> Vec<u64> {
    let mut found = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '#' && *c != '!' {
            continue;
        }
        let before_ok = i == 0 || matches!(chars[i - 1], '(' | ' ' | '[');
        let digits: String = chars[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if before_ok
            && !digits.is_empty()
            && let Ok(n) = digits.parse()
            && !found.contains(&n)
        {
            found.push(n);
        }
    }
    found
}

/// The largest bump `changes` call for (`None` when there are none).
pub(crate) fn bump_of(changes: &[Change]) -> Option<Bump> {
    changes
        .iter()
        .filter(|c| !c.is_release())
        .map(Change::bump)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventional_messages() {
        let c = Change::parse("a", "feat(ui)!: dark mode (#12)", "");
        assert_eq!(
            (c.kind.as_str(), c.scope.as_str(), c.breaking),
            ("feat", "ui", true)
        );
        assert_eq!(c.summary, "dark mode (#12)");
        assert_eq!(c.prs, [12]);
        let c = Change::parse("b", "Fix: typo", "BREAKING CHANGE: renamed x");
        assert_eq!(c.kind, "fix");
        assert!(c.breaking);
        let c = Change::parse("c", "Update README", "");
        assert_eq!((c.kind.as_str(), c.summary.as_str()), ("", "Update README"));
        let c = Change::parse("d", "Merge pull request #7 from ann/fix", "");
        assert_eq!(c.prs, [7]);
        let c = Change::parse("e", "feat(weird: no closing", "");
        assert_eq!(c.kind, "");
        assert!(Change::parse("f", "chore(release): v1.2.3", "").is_release());
        assert_eq!(Change::parse("g", "fix: issue#12 and !34", "").prs, [34]);
    }

    #[test]
    fn the_largest_bump_wins() {
        let changes = [
            Change::parse("a", "fix: x", ""),
            Change::parse("b", "feat: y", ""),
            Change::parse("c", "chore(release)!: v9", ""),
        ];
        assert_eq!(bump_of(&changes), Some(Bump::Minor));
        assert_eq!(bump_of(&[]), None);
    }
}
