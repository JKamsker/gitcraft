//! What `git push --porcelain` says of each ref it pushed, and why a refused one was refused.
//! Its lines are meant for programs and stay the same whatever language git speaks:
//! `<flag>\t<from>:<to>\t<summary> (<reason>)`, `!` being the flag of a refused ref.

/// One ref as `git push --porcelain` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RefLine {
    pub(super) flag: char,
    /// What was pushed: a ref (`refs/heads/main`), `HEAD` or a commit.
    pub(super) from: String,
    /// The ref it went to on the remote (`refs/heads/main`).
    pub(super) to: String,
    /// `[rejected]`, `[remote rejected]`, `[up to date]`…, without the brackets.
    pub(super) summary: String,
    /// What follows in brackets: `non-fast-forward`, `stale info`, a hook's words…
    pub(super) reason: String,
}

/// Why a push was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The remote has commits the branch doesn't (`non-fast-forward`, `fetch first`).
    Behind,
    /// A force push's lease no longer holds: the remote moved since it was looked at
    /// (`stale info`, `remote ref updated since checkout`).
    Moved,
    /// The remote's side said no (a hook, a protected branch): a force push would not help.
    Remote(String),
    /// Anything else (`already exists`, `atomic push failed`…).
    Other(String),
}

/// The ref lines in `out` (the push's whole output, its other lines skipped).
pub(super) fn parse(out: &str) -> Vec<RefLine> {
    out.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<RefLine> {
    let mut fields = line.splitn(3, '\t');
    let flag = fields.next()?;
    let refs = fields.next()?;
    let status = fields.next()?;
    let mut chars = flag.chars();
    let flag = chars.next().filter(|c| " +-*!=".contains(*c))?;
    if chars.next().is_some() {
        return None;
    }
    // A ref name holds no `:` (git refuses one), so the last `:` splits them.
    let (from, to) = refs.rsplit_once(':')?;
    let (summary, reason) = split_status(status.trim());
    Some(RefLine {
        flag,
        from: from.to_owned(),
        to: to.to_owned(),
        summary,
        reason,
    })
}

/// `[rejected] (non-fast-forward)` → (`rejected`, `non-fast-forward`).
fn split_status(status: &str) -> (String, String) {
    let (summary, rest) = match status.strip_prefix('[').and_then(|s| s.split_once(']')) {
        Some((summary, rest)) => (summary.to_owned(), rest.trim()),
        None => (String::new(), status),
    };
    let reason = rest
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(rest);
    (summary, reason.to_owned())
}

impl RefLine {
    /// Why it was refused, if it was.
    pub(super) fn refusal(&self) -> Option<Refusal> {
        if self.flag != '!' {
            return None;
        }
        Some(match (self.summary.as_str(), self.reason.as_str()) {
            ("rejected", "non-fast-forward" | "fetch first") => Refusal::Behind,
            ("rejected", "stale info" | "remote ref updated since checkout") => Refusal::Moved,
            ("remote rejected", reason) => Refusal::Remote(reason.to_owned()),
            (_, reason) => Refusal::Other(reason.to_owned()),
        })
    }
}

/// The refused line of a push of `branch` (`refs/heads/main`) or of the commit `commit`, with
/// why: that line, else the only refused one. `None` when nothing was refused (the push failed
/// before it reached the remote: no network, a sign-in…).
pub(super) fn refused(out: &str, branch: &str, commit: Option<&str>) -> Option<(RefLine, Refusal)> {
    let refused: Vec<(RefLine, Refusal)> = parse(out)
        .into_iter()
        .filter_map(|line| line.refusal().map(|why| (line, why)))
        .collect();
    let ours = refused
        .iter()
        .position(|(line, _)| line.from == branch || Some(line.from.as_str()) == commit);
    match ours {
        Some(i) => refused.into_iter().nth(i),
        None if refused.len() == 1 => refused.into_iter().next(),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEHIND: &str = "error: failed to push some refs to '/tmp/r.git'\n\
        hint: Updates were rejected because the remote contains work that you do not\n\
        To /tmp/r.git\n\
        !\trefs/heads/main:refs/heads/main\t[rejected] (fetch first)\n\
        Done";

    #[test]
    fn a_refused_line_says_why() {
        let lines = parse(BEHIND);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].flag, '!');
        assert_eq!(lines[0].from, "refs/heads/main");
        assert_eq!(lines[0].to, "refs/heads/main");
        assert_eq!(lines[0].summary, "rejected");
        assert_eq!(lines[0].reason, "fetch first");
        assert_eq!(lines[0].refusal(), Some(Refusal::Behind));
    }

    #[test]
    fn each_refusal_is_told_apart() {
        for (status, want) in [
            ("[rejected] (non-fast-forward)", Refusal::Behind),
            ("[rejected] (fetch first)", Refusal::Behind),
            ("[rejected] (stale info)", Refusal::Moved),
            (
                "[rejected] (remote ref updated since checkout)",
                Refusal::Moved,
            ),
            (
                "[remote rejected] (protected branch hook declined)",
                Refusal::Remote("protected branch hook declined".to_owned()),
            ),
            (
                "[remote rejected] (pre-receive hook declined)",
                Refusal::Remote("pre-receive hook declined".to_owned()),
            ),
            (
                "[rejected] (already exists)",
                Refusal::Other("already exists".to_owned()),
            ),
        ] {
            let out = format!("!\trefs/heads/x:refs/heads/x\t{status}");
            assert_eq!(
                parse_line(&out).map(|l| l.refusal()),
                Some(Some(want)),
                "{status}"
            );
        }
    }

    #[test]
    fn pushed_refs_are_not_refused() {
        for out in [
            " \trefs/heads/a:refs/heads/a\t1111111..2222222",
            "+\trefs/heads/a:refs/heads/a\t1111111...2222222 (forced update)",
            "*\trefs/heads/a:refs/heads/a\t[new branch]",
            "=\trefs/heads/a:refs/heads/a\t[up to date]",
            "-\t:refs/heads/a\t[deleted]",
        ] {
            assert_eq!(parse_line(out).map(|l| l.refusal()), Some(None), "{out}");
        }
    }

    /// Hints, `remote:` lines and the summary aren't ref lines, even with a tab in them.
    #[test]
    fn other_lines_are_skipped() {
        for out in [
            "To github.com:a/b.git",
            "Done",
            "hint: see\tgit push --help",
            "remote: error: GH006: Protected branch update failed",
            "!!\ta:b\t[rejected]",
            "!\tno-colon\t[rejected] (fetch first)",
        ] {
            assert_eq!(parse_line(out), None, "{out}");
        }
    }

    #[test]
    fn the_line_of_the_branch_pushed_is_the_one_taken() {
        let out = "!\trefs/heads/other:refs/heads/other\t[rejected] (non-fast-forward)\n\
            !\trefs/heads/main:refs/heads/main\t[remote rejected] (pre-receive hook declined)";
        let found = refused(out, "refs/heads/main", None);
        assert_eq!(
            found.as_ref().map(|(line, _)| line.to.as_str()),
            Some("refs/heads/main")
        );
        assert!(matches!(found, Some((_, Refusal::Remote(_)))));
        // Two others refused and neither is it: nothing to say about this branch.
        assert_eq!(refused(out, "refs/heads/third", None), None);
        // A commit pushed (a force push pushes the commit it was shown).
        let sha = "!\t1234abcd:refs/heads/main\t[rejected] (stale info)\n\
            !\trefs/heads/x:refs/heads/x\t[rejected] (stale info)";
        let found = refused(sha, "refs/heads/main", Some("1234abcd"));
        assert_eq!(
            found.map(|(line, why)| (line.from, why)),
            Some(("1234abcd".to_owned(), Refusal::Moved))
        );
    }

    #[test]
    fn a_push_that_never_reached_a_ref_is_no_refusal() {
        let out = "fatal: could not read Username for 'https://x': terminal prompts disabled";
        assert_eq!(refused(out, "refs/heads/main", None), None);
        let ok = "To /r.git\n \trefs/heads/main:refs/heads/main\t1..2\nDone";
        assert_eq!(refused(ok, "refs/heads/main", None), None);
    }
}
