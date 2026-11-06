//! Developer Certificate of Origin sign-offs: whether a repository asks for them, which commits
//! carry their author's, and the rewrite that adds them to the commits of a branch.

use std::path::Path;

use crate::git;

/// The files that say a repository asks for a DCO sign-off: the certificate itself, or the
/// settings of the DCO app (probot/dco), whose check refuses commits without one.
pub(crate) const DCO_FILES: [&str; 5] = ["DCO", "DCO.md", "DCO.txt", "DCO.rst", ".github/dco.yml"];

/// The key of gitcraft's own setting in the repository's git config: whether its commits are
/// signed off, as chosen in the commit box (unset: as the repository asks).
pub(crate) const SIGN_OFF_KEY: &str = "gitcraft.signOff";

/// What was found of the repository's sign-off rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rules {
    /// The file that asks for a sign-off, if one does (as committed at HEAD).
    pub file: Option<String>,
    /// What the commit box was last set to here (`gitcraft.signOff`).
    pub choice: Option<bool>,
}

/// Reads the repository's sign-off rules: the DCO file committed at HEAD, and the choice kept in
/// its git config. A repository without commits has no file yet.
pub(crate) fn read_rules(repo: &Path) -> Rules {
    let mut args = vec!["ls-tree", "--name-only", "HEAD", "--"];
    args.extend(DCO_FILES);
    let listed = git::run(Some(repo), &args).unwrap_or_default();
    // In the order of `DCO_FILES`, whichever order git lists them in.
    let file = DCO_FILES
        .iter()
        .find(|name| listed.lines().any(|line| line == **name))
        .map(|name| (*name).to_owned());
    // Exit 1: not set.
    let choice = git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--type=bool", "--get", SIGN_OFF_KEY],
        &[0, 1],
    )
    .ok()
    .and_then(|value| match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    });
    Rules { file, choice }
}

/// The git config command that keeps the commit box's choice for the repository.
pub(crate) fn choice_command(on: bool) -> Vec<String> {
    [
        "config",
        "--local",
        SIGN_OFF_KEY,
        if on { "true" } else { "false" },
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// Whether a check of a pull request is the DCO check (the DCO app's is named `DCO`).
pub(crate) fn is_dco_check(name: &str) -> bool {
    name.split(|c: char| !c.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("dco"))
}

/// One commit, as far as its sign-off goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Signed {
    pub hash: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    /// It has more than one parent.
    pub merge: bool,
    /// Its message has a `Signed-off-by:` of its author (by email).
    pub signed: bool,
}

/// Whether `message` has a `Signed-off-by: Name <email>` line for `email`, as the DCO check
/// reads it: anywhere in the message, the key in any case, the email compared without case.
pub(crate) fn signed_off_by(message: &str, email: &str) -> bool {
    let email = email.trim().to_lowercase();
    message.lines().any(|line| {
        let line = line.trim();
        let Some((key, value)) = line.split_once(':') else {
            return false;
        };
        if !key.trim().eq_ignore_ascii_case("signed-off-by") {
            return false;
        }
        let value = value.trim();
        let Some(open) = value.rfind('<') else {
            return false;
        };
        let Some(close) = value[open..].find('>') else {
            return false;
        };
        let name = value[..open].trim();
        let addr = value[open + 1..open + close].trim().to_lowercase();
        !name.is_empty() && !addr.is_empty() && addr == email
    })
}

/// Separates the fields of a commit in [`commits`]' format, and the commits.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// The commits of `range` (`a..b`), newest first, with whether each is signed off.
pub(crate) fn commits(repo: &Path, range: &str) -> Result<Vec<Signed>, String> {
    let format = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%s%x1f%B%x1e";
    let out = git::run(Some(repo), &["log", format, range, "--"])?;
    Ok(parse_commits(&out))
}

fn parse_commits(out: &str) -> Vec<Signed> {
    out.split(RECORD)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            let mut fields = record.splitn(6, FIELD);
            let hash = fields.next()?.trim().to_owned();
            if hash.is_empty() {
                return None;
            }
            let parents = fields.next()?;
            let author = fields.next()?.to_owned();
            let email = fields.next()?.to_owned();
            let summary = fields.next()?.to_owned();
            let message = fields.next().unwrap_or_default();
            Some(Signed {
                signed: signed_off_by(message, &email),
                merge: parents.split_whitespace().count() > 1,
                hash,
                summary,
                author,
                email,
            })
        })
        .collect()
}

/// The commits of `list` the DCO check would refuse: those without their author's sign-off.
/// Merges are left out, as the check leaves them out.
pub(crate) fn missing(list: &[Signed]) -> impl Iterator<Item = &Signed> {
    list.iter().filter(|c| !c.merge && !c.signed)
}

/// Where a branch is pushed: the remote, the branch there, and where that was last seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PushTarget {
    pub remote: String,
    /// `refs/heads/x` on the remote.
    pub remote_ref: String,
    /// The remote-tracking ref of it here (`refs/remotes/origin/x`), if there is one.
    pub tracking: Option<String>,
    /// Where `tracking` is: what the push expects to replace.
    pub seen: Option<String>,
}

/// The plan of signing off the commits of the checked-out branch since `base`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub branch: String,
    /// HEAD's commit now.
    pub head: String,
    /// Where the commits to rewrite start (not rewritten).
    pub base: String,
    pub commits: Vec<Signed>,
    pub push: Option<PushTarget>,
}

/// Plans signing off every commit of the checked-out branch since it left `base` (a commit or
/// ref: the pull request's base). Refused when there is nothing to do or the branch can't be
/// rewritten that way, with why.
pub(crate) fn plan(repo: &Path, base: &str) -> Result<Plan, String> {
    let branch = git::run_ok_codes(
        Some(repo),
        &["symbolic-ref", "-q", "--short", "HEAD"],
        &[0, 1],
    )?
    .trim()
    .to_owned();
    if branch.is_empty() {
        return Err("Check out the pull request's branch first: HEAD is detached".to_owned());
    }
    let head = git::head(repo);
    let merge_base = git::merge_base(repo, &head, base)
        .map_err(|e| format!("The branch and its base have no commit in common: {e}"))?;
    let commits = commits(repo, &format!("{merge_base}..{head}"))?;
    if commits.is_empty() {
        return Err(format!("{branch} has no commits of its own since its base"));
    }
    if commits.iter().any(|c| c.merge) {
        return Err(format!(
            "{branch} has merge commits since its base, which a rebase would flatten: sign it \
             off by hand"
        ));
    }
    if missing(&commits).next().is_none() {
        return Err(format!(
            "Every commit of {branch} is already signed off by its author"
        ));
    }
    let push = push_target(repo, &branch)?;
    Ok(Plan {
        branch,
        head,
        base: merge_base,
        commits,
        push,
    })
}

/// Where `branch` is pushed to (`branch.<name>.pushRemote` and its upstream decide), if
/// anywhere.
fn push_target(repo: &Path, branch: &str) -> Result<Option<PushTarget>, String> {
    let format = "--format=%(push:remotename)%00%(push:remoteref)%00%(push)%00\
                  %(upstream:remotename)%00%(upstream:remoteref)";
    let out = git::run(
        Some(repo),
        &["for-each-ref", format, &format!("refs/heads/{branch}")],
    )?;
    let line = out.lines().next().unwrap_or_default();
    let parts: Vec<&str> = line.split('\0').map(str::trim).collect();
    let part = |i: usize| parts.get(i).copied().unwrap_or_default();
    let remote = part(0).to_owned();
    if remote.is_empty() {
        return Ok(None);
    }
    // git leaves the branch pushed to unsaid for `push.default=simple`: the upstream's where
    // that is on the same remote, else the branch of the same name.
    let remote_ref = if !part(1).is_empty() {
        part(1).to_owned()
    } else if part(3) == remote && !part(4).is_empty() {
        part(4).to_owned()
    } else {
        format!("refs/heads/{branch}")
    };
    let tracking = Some(part(2)).filter(|t| !t.is_empty()).map(str::to_owned);
    let seen = tracking
        .as_deref()
        .and_then(|t| git::resolve(repo, t).ok())
        .filter(|s| !s.is_empty());
    Ok(Some(PushTarget {
        remote,
        remote_ref,
        tracking,
        seen,
    }))
}

impl Plan {
    /// The steps that sign the commits off and push them (only while the branch and HEAD are
    /// where the plan saw them, and the remote's branch where it was last fetched).
    pub(crate) fn steps(&self) -> Vec<Vec<String>> {
        use crate::repo::operations::{expect_ref_at, expect_symref};
        let local = format!("refs/heads/{}", self.branch);
        let mut steps = vec![
            expect_symref("HEAD", &local),
            expect_ref_at(&local, &self.head),
            strings(&["rebase", "--signoff", "--force-rebase", &self.base]),
        ];
        if let Some(push) = &self.push {
            let lease = match &push.seen {
                Some(seen) => format!("--force-with-lease={}:{seen}", push.remote_ref),
                None => format!("--force-with-lease={}:", push.remote_ref),
            };
            steps.push(strings(&[
                "push",
                &lease,
                "--",
                &push.remote,
                &format!("{local}:{}", push.remote_ref),
            ]));
        }
        steps
    }

    /// The steps that put the commits as they were back, here and on the remote: only while the
    /// branch (and the remote's, as last seen) are still at the signed-off commits.
    pub(crate) fn undo_steps(&self) -> Vec<Vec<String>> {
        use crate::repo::operations::{expect_ref, expect_symref};
        let local = format!("refs/heads/{}", self.branch);
        let mut steps = vec![expect_symref("HEAD", &local), expect_ref(&local)];
        if let Some(PushTarget {
            tracking: Some(tracking),
            ..
        }) = &self.push
        {
            steps.push(expect_ref(tracking));
        }
        steps.push(strings(&["reset", "--keep", &self.head]));
        if let Some(push) = &self.push {
            steps.push(back_push(push, &local));
        }
        steps
    }

    /// The steps that sign them off again after an undo.
    pub(crate) fn redo_steps(&self) -> Vec<Vec<String>> {
        use crate::repo::operations::{expect_ref, expect_symref};
        let local = format!("refs/heads/{}", self.branch);
        let mut steps = vec![
            expect_symref("HEAD", &local),
            expect_ref(&local),
            strings(&["rebase", "--signoff", "--force-rebase", &self.base]),
        ];
        if let Some(push) = &self.push {
            steps.push(back_push(push, &local));
        }
        steps
    }

    /// The commits the plan signs off, which their authors haven't.
    pub(crate) fn unsigned(&self) -> Vec<&Signed> {
        missing(&self.commits).collect()
    }

    /// Those of them by someone else than `email`: the check wants their own sign-off.
    pub(crate) fn by_others(&self, email: &str) -> Vec<&Signed> {
        let email = email.trim().to_lowercase();
        self.unsigned()
            .into_iter()
            .filter(|c| c.email.trim().to_lowercase() != email)
            .collect()
    }
}

/// A push of `local` to `push`'s branch that replaces what is there only if it is still what was
/// last fetched of it (the remote-tracking branch, checked before by the undo).
fn back_push(push: &PushTarget, local: &str) -> Vec<String> {
    let lease = match &push.tracking {
        Some(_) => format!("--force-with-lease={}", push.remote_ref),
        None => format!("--force-with-lease={}:", push.remote_ref),
    };
    strings(&[
        "push",
        &lease,
        "--",
        &push.remote,
        &format!("{local}:{}", push.remote_ref),
    ])
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_off_is_the_authors_by_email_in_any_case() {
        let msg = "fix: x\n\nbody\n\nSigned-off-by: Ann Lee <Ann@Example.org>\n";
        assert!(signed_off_by(msg, "ann@example.org"));
        assert!(signed_off_by(msg, " ANN@example.org "));
        assert!(!signed_off_by(msg, "bob@example.org"));
        assert!(signed_off_by("x\n\nsigned-off-by: Ann <ann@e>", "ann@e"));
        assert!(signed_off_by("x\n\nSigned-off-by:Ann <ann@e>", "ann@e"));
    }

    #[test]
    fn a_sign_off_needs_a_name_and_an_email() {
        assert!(!signed_off_by("x\n\nSigned-off-by: <ann@e>", "ann@e"));
        assert!(!signed_off_by("x\n\nSigned-off-by: Ann", "ann@e"));
        assert!(!signed_off_by("x\n\nSigned-off-by: Ann <>", ""));
        assert!(!signed_off_by("x\n\nCo-authored-by: Ann <ann@e>", "ann@e"));
        assert!(!signed_off_by("", "ann@e"));
    }

    #[test]
    fn unicode_names_and_emails_are_read() {
        let msg = "feat: ü\n\nSigned-off-by: Zoë Ångström <zoë@exämple.org>";
        assert!(signed_off_by(msg, "ZOË@exämple.org"));
    }

    #[test]
    fn the_log_is_read_into_commits() {
        let out = "\
aaa\u{1f}p1\u{1f}Ann\u{1f}ann@e\u{1f}one\u{1f}one\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
bbb\u{1f}p1 p2\u{1f}Bob\u{1f}bob@e\u{1f}Merge\u{1f}Merge\n\u{1e}\n\
ccc\u{1f}p1\u{1f}Bob\u{1f}bob@e\u{1f}two\u{1f}two\n\nSigned-off-by: Ann <ann@e>\n\u{1e}";
        let list = parse_commits(out);
        assert_eq!(list.len(), 3);
        assert!(list[0].signed && !list[0].merge);
        assert!(list[1].merge);
        assert!(!list[2].signed, "signed by someone else than the author");
        let missing: Vec<&str> = missing(&list).map(|c| c.hash.as_str()).collect();
        assert_eq!(missing, ["ccc"]);
    }

    #[test]
    fn dco_checks_are_known_by_name() {
        assert!(is_dco_check("DCO"));
        assert!(is_dco_check("dco"));
        assert!(is_dco_check("probot/dco"));
        assert!(is_dco_check("DCO check"));
        assert!(is_dco_check("ci / DCO"));
        assert!(!is_dco_check("deco"));
        assert!(!is_dco_check("build"));
    }
}
