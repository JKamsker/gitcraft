//! Developer Certificate of Origin sign-offs: whether a repository asks for them, which commits
//! carry one the DCO check takes, and the rewrite that adds them to the commits of a branch.

use std::path::Path;

use crate::git;
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};

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
    /// What the commit box was set to here (`gitcraft.signOff`); unset: as the repository asks.
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
    // Exit 1: not set. `--bool`, which gits older than 2.18 (`--type`) know too.
    let choice = git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--bool", "--get", SIGN_OFF_KEY],
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

/// The git config command that keeps the commit box's choice for the repository, or takes it
/// out (`None`), so that the box follows what the repository asks again.
pub(crate) fn choice_command(choice: Option<bool>) -> Vec<String> {
    let args: &[&str] = match choice {
        Some(on) => &[
            "config",
            "--local",
            SIGN_OFF_KEY,
            if on { "true" } else { "false" },
        ],
        None => &["config", "--local", "--unset-all", SIGN_OFF_KEY],
    };
    strings(args)
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
    /// Its author is a bot (`dependabot[bot]`), which the DCO check leaves alone.
    pub bot: bool,
    /// Its message has a sign-off the DCO check takes: its author's or its committer's.
    pub signed: bool,
}

impl Signed {
    /// Whether the DCO check refuses it.
    pub(crate) fn refused(&self) -> bool {
        !self.merge && !self.bot && !self.signed
    }
}

/// The `Name <email>` of each `Signed-off-by:` line of `message`, read as the DCO app reads
/// them (`^Signed-off-by: (.*) <(.*)>\s*$`, any case of the key).
fn sign_offs(message: &str) -> impl Iterator<Item = (&str, &str)> {
    message.lines().filter_map(|line| {
        let line = line.trim_end();
        let key = line.get(..15)?;
        if !key.eq_ignore_ascii_case("signed-off-by: ") {
            return None;
        }
        let rest = line[15..].strip_suffix('>')?;
        let (name, email) = rest.rsplit_once(" <")?;
        Some((name, email))
    })
}

/// Whether `message` is signed off by one of `people` (`(name, email)`), as the DCO check reads
/// it: the name and the email both, without case.
pub(crate) fn signed_off_by(message: &str, people: &[(&str, &str)]) -> bool {
    sign_offs(message).any(|(name, email)| {
        people.iter().any(|(n, e)| {
            !e.trim().is_empty()
                && name.trim().eq_ignore_ascii_case(n.trim())
                && email.trim().eq_ignore_ascii_case(e.trim())
        })
    })
}

/// Separates the fields of a commit in [`commits`]' format, and the commits.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// The commits of `range` (`a..b`), newest first, with whether each is signed off.
pub(crate) fn commits(repo: &Path, range: &str) -> Result<Vec<Signed>, String> {
    let format = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%s%x1f%B%x1e";
    let out = git::run(Some(repo), &["log", format, range, "--"])?;
    Ok(parse_commits(&out))
}

fn parse_commits(out: &str) -> Vec<Signed> {
    out.split(RECORD)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            let mut fields = record.splitn(8, FIELD);
            let hash = fields.next()?.trim().to_owned();
            if hash.is_empty() {
                return None;
            }
            let parents = fields.next()?;
            let author = fields.next()?.to_owned();
            let email = fields.next()?.to_owned();
            let committer = fields.next()?;
            let committer_email = fields.next()?;
            let summary = fields.next()?.to_owned();
            let message = fields.next().unwrap_or_default();
            let people = [
                (author.as_str(), email.as_str()),
                (committer, committer_email),
            ];
            Some(Signed {
                signed: signed_off_by(message, &people),
                merge: parents.split_whitespace().count() > 1,
                bot: author.ends_with("[bot]"),
                hash,
                summary,
                author,
                email,
            })
        })
        .collect()
}

/// The commits of `list` the DCO check would refuse.
pub(crate) fn missing(list: &[Signed]) -> impl Iterator<Item = &Signed> {
    list.iter().filter(|c| c.refused())
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

impl PushTarget {
    /// `origin/x`, as people read it.
    pub(crate) fn label(&self) -> String {
        let there = self
            .remote_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(&self.remote_ref);
        format!("{}/{there}", self.remote)
    }
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

/// Plans signing off every commit of the checked-out branch since it left `base` (the pull
/// request's base). `expected` is the pull request's latest commit: the branch must be at it,
/// so that the commits rewritten are the pull request's. Refused, with why, when there is
/// nothing to do or the branch can't be rewritten that way.
pub(crate) fn plan(repo: &Path, base: &str, expected: Option<&str>) -> Result<Plan, String> {
    let full = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?
        .trim()
        .to_owned();
    let Some(branch) = full.strip_prefix("refs/heads/").map(str::to_owned) else {
        return Err(crate::i18n::text("repo-workflows-dco-plan-d866c6e0").to_owned());
    };
    let head = git::head(repo);
    if let Some(expected) = expected.filter(|e| *e != head) {
        return Err(crate::i18n::message(
            "repo-workflows-dco-plan-589b41e2",
            &[
                ("branch", (branch).clone().into()),
                (
                    "crate-repo-short-head-1",
                    (crate::repo::short(&head)).into(),
                ),
                (
                    "crate-repo-short-expected-2",
                    (crate::repo::short(expected)).into(),
                ),
            ],
        ));
    }
    let dirty = git::run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    if !dirty.trim().is_empty() {
        return Err(crate::i18n::text("repo-workflows-dco-plan-cfb9aeb9").to_owned());
    }
    let merge_base = git::merge_base(repo, &head, base).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-dco-plan-0ac1ebd9",
            &[("e", (e).clone().into())],
        )
    })?;
    let commits = commits(repo, &format!("{merge_base}..{head}"))?;
    if commits.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-dco-plan-4c71d659",
            &[("branch", (branch).clone().into())],
        ));
    }
    if commits.iter().any(|c| c.merge) {
        return Err(crate::i18n::message(
            "repo-workflows-dco-plan-d7cbaa06",
            &[
                ("branch", (branch).clone().into()),
                (
                    "crate-repo-short-merge-base-1",
                    (crate::repo::short(&merge_base)).into(),
                ),
            ],
        ));
    }
    if missing(&commits).next().is_none() {
        return Err(crate::i18n::message(
            "repo-workflows-dco-plan-a16c4b9b",
            &[("branch", (branch).clone().into())],
        ));
    }
    let push = push_target(repo, &branch)?;
    if let Some(seen) = push.as_ref().and_then(|p| p.seen.as_deref())
        && !is_ancestor(repo, seen, &head)?
    {
        let label = push.as_ref().map(PushTarget::label).unwrap_or_default();
        return Err(crate::i18n::message(
            "repo-workflows-dco-plan-927db158",
            &[
                ("label", (label).clone().into()),
                ("branch", (branch).clone().into()),
            ],
        ));
    }
    Ok(Plan {
        branch,
        head,
        base: merge_base,
        commits,
        push,
    })
}

/// Whether `ancestor` is in the history of `of`: none of its commits are missing there.
fn is_ancestor(repo: &Path, ancestor: &str, of: &str) -> Result<bool, String> {
    let count = git::run(
        Some(repo),
        &["rev-list", "--count", &format!("{of}..{ancestor}"), "--"],
    )?;
    Ok(count.trim() == "0")
}

/// Where `branch` is pushed to (`branch.<name>.pushRemote` and its upstream decide), if
/// anywhere, and the remote-tracking ref of that branch.
fn push_target(repo: &Path, branch: &str) -> Result<Option<PushTarget>, String> {
    let format = "--format=%(push:remotename)%00%(push:remoteref)%00%(push)%00\
                  %(upstream:remotename)%00%(upstream:remoteref)%00%(upstream)";
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
    let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    // git leaves the branch pushed to unsaid for `push.default=simple`: the upstream's where
    // that is on the same remote (and its remote-tracking ref with it), else the branch of the
    // same name.
    let (remote_ref, tracking) = if !part(1).is_empty() {
        (part(1).to_owned(), some(part(2)))
    } else if part(3) == remote && !part(4).is_empty() {
        (part(4).to_owned(), some(part(5)))
    } else {
        let tracking = format!("refs/remotes/{remote}/{branch}");
        let known = git::resolve(repo, &tracking).is_ok();
        (format!("refs/heads/{branch}"), known.then_some(tracking))
    };
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

/// The rebase that signs off the commits after `base`, on the branch checked out only (other
/// branches `rebase.updateRefs` would move along aren't put back by undo).
fn signoff_rebase(base: &str) -> Vec<String> {
    strings(&[
        "-c",
        "rebase.updateRefs=false",
        "rebase",
        "--signoff",
        "--force-rebase",
        base,
    ])
}

impl Plan {
    fn local(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }

    /// The steps that sign the commits off and push them (only while the branch and HEAD are
    /// where the plan saw them, and the remote's branch where it was last fetched).
    pub(crate) fn steps(&self) -> Vec<Vec<String>> {
        let local = self.local();
        let mut steps = vec![
            expect_symref("HEAD", &local),
            expect_ref_at(&local, &self.head),
            signoff_rebase(&self.base),
        ];
        steps.extend(self.push_steps());
        steps
    }

    /// The push of the signed-off branch: replacing what the remote has only if it is still
    /// what was last fetched of it.
    pub(crate) fn push_steps(&self) -> Vec<Vec<String>> {
        let Some(push) = &self.push else {
            return Vec::new();
        };
        let lease = format!(
            "--force-with-lease={}:{}",
            push.remote_ref,
            push.seen.as_deref().unwrap_or_default()
        );
        vec![strings(&[
            "push",
            &lease,
            "--",
            &push.remote,
            &format!("{}:{}", self.local(), push.remote_ref),
        ])]
    }

    /// The steps that put the commits as they were back, on the remote first and then here:
    /// only while the branch (and the remote's, as last seen) are still at the signed-off
    /// commits.
    pub(crate) fn undo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local();
        let mut steps = vec![expect_symref("HEAD", &local), expect_ref(&local)];
        if let Some(PushTarget {
            tracking: Some(tracking),
            ..
        }) = &self.push
        {
            steps.push(expect_ref(tracking));
        }
        if let Some(push) = &self.push {
            steps.push(back_push(push, &self.head));
        }
        steps.push(strings(&["reset", "--keep", &self.head]));
        steps
    }

    /// The steps that sign them off again after an undo.
    pub(crate) fn redo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local();
        let mut steps = vec![
            expect_symref("HEAD", &local),
            expect_ref(&local),
            signoff_rebase(&self.base),
        ];
        if let Some(push) = &self.push {
            steps.push(back_push(push, &local));
        }
        steps
    }

    /// The undo of a rewrite whose push failed: the branch back where it was, here only.
    pub(crate) fn local_undo(&self, now: &str) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
        let local = self.local();
        let undo = vec![
            expect_symref("HEAD", &local),
            expect_ref_at(&local, now),
            strings(&["reset", "--keep", &self.head]),
        ];
        let redo = vec![
            expect_symref("HEAD", &local),
            expect_ref_at(&local, &self.head),
            strings(&["reset", "--keep", now]),
        ];
        (undo, redo)
    }

    /// The commits the plan signs off, which the DCO check refuses now.
    pub(crate) fn unsigned(&self) -> Vec<&Signed> {
        missing(&self.commits).collect()
    }

    /// Those of them by someone else than `email`.
    pub(crate) fn by_others(&self, email: &str) -> Vec<&Signed> {
        self.unsigned()
            .into_iter()
            .filter(|c| !c.email.trim().eq_ignore_ascii_case(email.trim()))
            .collect()
    }
}

/// A push of `what` (a commit or a ref) to `push`'s branch that replaces what is there only if
/// it is still what was last fetched of it (the remote-tracking branch, which the undo checks
/// first).
fn back_push(push: &PushTarget, what: &str) -> Vec<String> {
    let lease = match &push.tracking {
        Some(_) => format!("--force-with-lease={}", push.remote_ref),
        None => format!("--force-with-lease={}:", push.remote_ref),
    };
    strings(&[
        "push",
        &lease,
        "--",
        &push.remote,
        &format!("{what}:{}", push.remote_ref),
    ])
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: (&str, &str) = ("Ann Lee", "Ann@Example.org");

    #[test]
    fn a_sign_off_counts_with_the_name_and_email_in_any_case() {
        let msg = "fix: x\n\nbody\n\nSigned-off-by: Ann Lee <ann@example.org>\n";
        assert!(signed_off_by(msg, &[ANN]));
        assert!(signed_off_by(msg, &[("ANN LEE", "ann@EXAMPLE.org")]));
        assert!(!signed_off_by(msg, &[("Bob", "bob@example.org")]));
        assert!(signed_off_by(
            "x\n\nsigned-off-by: Ann Lee <ann@example.org>",
            &[ANN]
        ));
    }

    /// As the DCO app reads them: one space after the colon, the line ending with `>`, and the
    /// name too.
    #[test]
    fn a_sign_off_is_read_as_the_dco_check_reads_it() {
        assert!(!signed_off_by(
            "x\n\nSigned-off-by:Ann Lee <ann@example.org>",
            &[ANN]
        ));
        assert!(!signed_off_by(
            "x\n\n  Signed-off-by: Ann Lee <ann@example.org>",
            &[ANN]
        ));
        assert!(!signed_off_by(
            "x\n\nSigned-off-by: Ann Lee <ann@example.org> ok",
            &[ANN]
        ));
        assert!(!signed_off_by(
            "x\n\nSigned-off-by: Ann <ann@example.org>",
            &[ANN]
        ));
        assert!(signed_off_by(
            "x\n\nSigned-off-by: Ann Lee <ann@example.org>   ",
            &[ANN]
        ));
        assert!(!signed_off_by("x\n\nSigned-off-by: Ann Lee", &[ANN]));
        assert!(!signed_off_by(
            "x\n\nCo-authored-by: Ann Lee <ann@example.org>",
            &[ANN]
        ));
        assert!(!signed_off_by("", &[ANN]));
        assert!(!signed_off_by(
            "x\n\nSigned-off-by: Ann Lee <>",
            &[("Ann Lee", "")]
        ));
    }

    #[test]
    fn unicode_names_and_emails_are_read() {
        let msg = "feat: ü\n\nSigned-off-by: Zoë Ångström <zoë@exämple.org>";
        assert!(signed_off_by(msg, &[("Zoë Ångström", "zoë@exämple.org")]));
    }

    #[test]
    fn the_log_is_read_into_commits() {
        let out = "\
aaa\u{1f}p1\u{1f}Ann\u{1f}ann@e\u{1f}Ann\u{1f}ann@e\u{1f}one\u{1f}one\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
bbb\u{1f}p1 p2\u{1f}Bob\u{1f}bob@e\u{1f}Bob\u{1f}bob@e\u{1f}Merge\u{1f}Merge\n\u{1e}\n\
ccc\u{1f}p1\u{1f}Bob\u{1f}bob@e\u{1f}Bob\u{1f}bob@e\u{1f}two\u{1f}two\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
ddd\u{1f}p1\u{1f}Bob\u{1f}bob@e\u{1f}Ann\u{1f}ann@e\u{1f}three\u{1f}three\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
eee\u{1f}p1\u{1f}dependabot[bot]\u{1f}b@e\u{1f}x\u{1f}x@e\u{1f}bump\u{1f}bump\n\u{1e}";
        let list = parse_commits(out);
        assert_eq!(list.len(), 5);
        assert!(list[0].signed && !list[0].merge);
        assert!(list[1].merge);
        assert!(
            !list[2].signed,
            "signed by someone else than its author and committer"
        );
        assert!(list[3].signed, "signed by its committer");
        assert!(list[4].bot);
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

    #[test]
    fn the_choice_is_kept_or_taken_out() {
        assert_eq!(
            choice_command(Some(true)),
            ["config", "--local", SIGN_OFF_KEY, "true"]
        );
        assert_eq!(
            choice_command(None),
            ["config", "--local", "--unset-all", SIGN_OFF_KEY]
        );
    }
}
