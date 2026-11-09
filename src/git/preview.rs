//! What a destructive operation would do, read before it runs: the commits that leave a branch
//! (and whether another ref still has each), whether they were pushed, and the uncommitted files
//! that would be lost. Read-only; every call goes through [`run`], so it works the same for a
//! repository in WSL or over SSH.

use std::path::Path;

use super::status::parse_status;
use super::{END_OPTS, FS, NO_SIGNATURE, UTF8, run};

/// How many commits, and files, a preview lists; the rest are only counted.
pub const PREVIEW_SHOWN: usize = 25;

/// The commits reachable from `tips` and not from `keep` (from `tips` alone when `keep` is
/// `None`, as for a branch whose history shares nothing with HEAD's, or with HEAD unborn).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub tips: Vec<String>,
    pub keep: Option<String>,
}

impl Range {
    pub fn new(tip: &str, keep: Option<&str>) -> Self {
        Range {
            tips: vec![tip.to_owned()],
            keep: keep.filter(|k| !k.is_empty()).map(str::to_owned),
        }
    }

    /// The range with the commits of `tip` too (a remote branch deleted with the local one).
    pub fn and_tip(mut self, tip: &str) -> Self {
        if !self.tips.iter().any(|t| t == tip) {
            self.tips.push(tip.to_owned());
        }
        self
    }

    /// `tip… ^keep`, after [`END_OPTS`] (which must come before them).
    fn revisions(&self) -> Vec<String> {
        let mut revs = self.tips.clone();
        revs.extend(self.keep.as_ref().map(|k| format!("^{k}")));
        revs
    }
}

/// Which uncommitted files an operation throws away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LostFiles {
    None,
    /// What `reset --hard` to the commit given loses: every uncommitted change to a tracked
    /// file, staged or not, and the untracked or ignored files the commit's tree has a file of
    /// the same path for (written over). Other untracked files stay. `None`: tracked changes
    /// only.
    HardReset(Option<String>),
    /// These paths, as the operation names them (discarding one file).
    Listed(Vec<String>),
}

/// What to read about an operation before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewAsk {
    /// The commits that leave the branch.
    pub leaving: Option<Range>,
    /// The full names of the refs the operation moves or deletes (`refs/heads/x`): a commit
    /// only they have is _only here_, not _still on_ them.
    pub moving: Vec<String>,
    /// The commits whose ids change (a reset's or a rebase's), checked against `upstream`.
    pub rewritten: Option<Range>,
    /// The full name of the remote-tracking branch the branch is pushed to.
    pub upstream: Option<String>,
    pub files: LostFiles,
}

impl PreviewAsk {
    /// Nothing to read: only what the dialog knows already (the commands, undo).
    pub fn nothing() -> Self {
        PreviewAsk {
            leaving: None,
            moving: Vec::new(),
            rewritten: None,
            upstream: None,
            files: LostFiles::None,
        }
    }

    /// A reset of `target` (a branch, or `HEAD` when detached) from `head` to `to`; `upstream`
    /// the remote-tracking branch `target` is pushed to. Only a hard one loses files.
    pub fn reset(target: &str, head: &str, to: &str, upstream: Option<String>, hard: bool) -> Self {
        let range = Range::new(head, Some(to));
        PreviewAsk {
            leaving: Some(range.clone()),
            moving: vec![branch_ref(target)],
            rewritten: Some(range),
            upstream,
            files: if hard {
                LostFiles::HardReset(Some(to.to_owned()))
            } else {
                LostFiles::None
            },
        }
    }

    /// Deleting the refs `moving` (full names), at `tips`: the commits leaving are those HEAD
    /// (at `head`, empty when unborn) hasn't.
    pub fn delete(tips: &[&str], head: &str, moving: Vec<String>) -> Self {
        let range = (tips.iter().skip(1)).fold(
            Range::new(tips.first().copied().unwrap_or("HEAD"), Some(head)),
            |range, tip| range.and_tip(tip),
        );
        PreviewAsk {
            leaving: Some(range),
            moving,
            ..PreviewAsk::nothing()
        }
    }

    /// Dropping the commit `hash` (whose parent is `parent`) from `branch`, at `head`, with a
    /// rebase that rewrites every commit after it.
    pub fn drop_commit(
        hash: &str,
        parent: &str,
        branch: &str,
        head: &str,
        upstream: Option<String>,
    ) -> Self {
        PreviewAsk {
            leaving: Some(Range::new(hash, Some(parent))),
            moving: vec![branch_ref(branch)],
            rewritten: Some(Range::new(head, Some(parent))),
            upstream,
            files: LostFiles::None,
        }
    }

    /// Replaying the commits unique to this branch. IDs change even where patches survive.
    pub fn rebase(branch: &str, head: &str, onto: &str, upstream: Option<String>) -> Self {
        let range = Range::new(head, Some(onto));
        Self {
            leaving: Some(range.clone()),
            rewritten: Some(range),
            upstream,
            moving: vec![branch_ref(branch)],
            files: LostFiles::None,
        }
    }

    /// Discarding the changes to these files, or deleting them.
    pub fn files(paths: Vec<String>) -> Self {
        PreviewAsk {
            files: LostFiles::Listed(paths),
            ..PreviewAsk::nothing()
        }
    }

    /// Whether there is anything to read from git at all.
    pub fn reads_git(&self) -> bool {
        self.leaving.is_some()
            || (self.rewritten.is_some() && self.upstream.is_some())
            || matches!(self.files, LostFiles::HardReset(_))
    }
}

/// The full name of the local branch `name`, or `HEAD` for a detached HEAD.
fn branch_ref(name: &str) -> String {
    if name == "HEAD" {
        name.to_owned()
    } else {
        format!("refs/heads/{name}")
    }
}

/// Where else a commit that leaves the branch is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kept {
    /// On no other branch, remote branch or tag: only the reflog has it after this.
    OnlyHere,
    /// On this branch, remote branch or tag (`main`, `origin/main`, `tag v1.0`).
    On(String),
    /// On some other ref, which git didn't name.
    Elsewhere,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeavingCommit {
    pub hash: String,
    pub summary: String,
    pub kept: Kept,
}

/// The commits of a [`PreviewAsk::rewritten`] range that `upstream` already has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pushed {
    /// `origin/main`.
    pub upstream: String,
    pub commits: usize,
}

/// What an operation would do, as read from git.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    /// The first [`PREVIEW_SHOWN`] commits leaving, newest first.
    pub leaving: Vec<LeavingCommit>,
    pub leaving_total: usize,
    /// How many of all the leaving commits no other ref has.
    pub only_here_total: usize,
    pub pushed: Option<Pushed>,
    /// The first [`PREVIEW_SHOWN`] files lost.
    pub lost_files: Vec<String>,
    pub lost_total: usize,
}

/// Reads what `ask` wants to know about the repository `repo`.
pub fn preview(repo: &Path, ask: &PreviewAsk) -> Result<Preview, String> {
    let mut found = Preview::default();
    if let Some(range) = &ask.leaving {
        let (leaving, total, only_here) = leaving_commits(repo, range, &ask.moving)?;
        found.leaving = leaving;
        found.leaving_total = total;
        found.only_here_total = only_here;
    }
    if let (Some(range), Some(upstream)) = (&ask.rewritten, &ask.upstream) {
        found.pushed = pushed(repo, range, upstream)?;
    }
    let lost = lost_files(repo, &ask.files)?;
    found.lost_total = lost.len();
    found.lost_files = lost.into_iter().take(PREVIEW_SHOWN).collect();
    Ok(found)
}

/// The first commits of `range` with where else each is, how many there are, and how many of
/// them only the refs in `moving` have.
fn leaving_commits(
    repo: &Path,
    range: &Range,
    moving: &[String],
) -> Result<(Vec<LeavingCommit>, usize, usize), String> {
    let total = count(repo, &[], &range.revisions())?;
    if total == 0 {
        return Ok((Vec::new(), 0, 0));
    }
    let shown = first_commits(repo, range)?;
    let others = other_refs(moving);
    let only_here_total = count(repo, &others, &range.revisions())?;
    let only_here = if only_here_total == 0 {
        Vec::new()
    } else {
        // The commits only here come in the order of the range, so the first of them include
        // every one among the commits shown.
        hashes(repo, &others, &range.revisions(), PREVIEW_SHOWN)?
    };
    let elsewhere: Vec<&str> = shown
        .iter()
        .map(|(hash, _)| hash.as_str())
        .filter(|h| !only_here.iter().any(|o| o == h))
        .collect();
    let names = ref_names(repo, &elsewhere, moving)?;
    let leaving = shown
        .into_iter()
        .map(|(hash, summary)| {
            let kept = if only_here.contains(&hash) {
                Kept::OnlyHere
            } else {
                names
                    .iter()
                    .find(|(h, _)| *h == hash)
                    .and_then(|(_, name)| name.clone())
                    .map_or(Kept::Elsewhere, Kept::On)
            };
            LeavingCommit {
                hash,
                summary,
                kept,
            }
        })
        .collect();
    Ok((leaving, total, only_here_total))
}

/// The revision-walk arguments that leave out what any branch, remote branch or tag has, other
/// than the refs in `moving` (and the stash, notes, HEAD: none of them keeps a commit for the
/// user). Before [`END_OPTS`]; the `--not` at the end turns the revisions after them positive
/// again.
fn other_refs(moving: &[String]) -> Vec<String> {
    let excluded = |prefix: &str| {
        moving
            .iter()
            .filter_map(|r| r.strip_prefix(prefix))
            .map(|name| format!("--exclude={name}"))
            .collect::<Vec<_>>()
    };
    let mut args = vec!["--not".to_owned()];
    args.extend(excluded("refs/heads/"));
    args.push("--branches".to_owned());
    args.extend(excluded("refs/remotes/"));
    // `origin/HEAD` only points at another remote branch, and dangles once that is deleted.
    args.push("--exclude=*/HEAD".to_owned());
    args.push("--remotes".to_owned());
    args.extend(excluded("refs/tags/"));
    args.push("--tags".to_owned());
    args.push("--not".to_owned());
    args
}

/// How many commits `revs` (after `walk`) has.
fn count(repo: &Path, walk: &[String], revs: &[String]) -> Result<usize, String> {
    let mut args = vec!["rev-list", "--count"];
    args.extend(walk.iter().map(String::as_str));
    args.push(END_OPTS);
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    let out = run(Some(repo), &args)?;
    out.trim().parse().map_err(|_| {
        crate::i18n::message(
            "backend-git-counted-no-commits-e38477",
            &[("detail", (out.trim()).to_string().into())],
        )
    })
}

/// The first `max` commits of `revs` (after `walk`), newest first.
fn hashes(
    repo: &Path,
    walk: &[String],
    revs: &[String],
    max: usize,
) -> Result<Vec<String>, String> {
    let max = format!("--max-count={max}");
    let mut args = vec!["rev-list", &max];
    args.extend(walk.iter().map(String::as_str));
    args.push(END_OPTS);
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    Ok(run(Some(repo), &args)?.lines().map(str::to_owned).collect())
}

/// The first [`PREVIEW_SHOWN`] commits of `range`, newest first: hash and summary.
fn first_commits(repo: &Path, range: &Range) -> Result<Vec<(String, String)>, String> {
    let max = format!("--max-count={PREVIEW_SHOWN}");
    let format = format!("--format=%H{FS}%s");
    let revs = range.revisions();
    let mut args = vec!["log", NO_SIGNATURE, UTF8, &max, &format, END_OPTS];
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    Ok(parse_commits(&run(Some(repo), &args)?))
}

fn parse_commits(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|line| {
            let (hash, summary) = line.split_once(FS)?;
            Some((hash.to_owned(), summary.to_owned()))
        })
        .collect()
}

/// A branch, remote branch or tag that has each of `hashes`, but not one of `moving`: `None`
/// for a commit git found none for. Branches first, then remote branches, then tags: git would
/// otherwise name a tag over the branch a person knows the commit by.
fn ref_names(
    repo: &Path,
    hashes: &[&str],
    moving: &[String],
) -> Result<Vec<(String, Option<String>)>, String> {
    let mut excluded: Vec<String> = moving.iter().map(|r| format!("--exclude={r}")).collect();
    excluded.push("--exclude=refs/remotes/*/HEAD".to_owned());
    let mut names: Vec<(String, Option<String>)> =
        hashes.iter().map(|h| ((*h).to_owned(), None)).collect();
    for (refs, kind) in [
        ("--refs=refs/heads/*", RefKind::Branch),
        ("--refs=refs/remotes/*", RefKind::Remote),
        ("--refs=refs/tags/*", RefKind::Tag),
    ] {
        let unnamed: Vec<&str> = (names.iter())
            .filter(|(_, name)| name.is_none())
            .map(|(h, _)| h.as_str())
            .collect();
        if unnamed.is_empty() {
            break;
        }
        let mut args = vec!["name-rev", refs];
        args.extend(excluded.iter().map(String::as_str));
        args.extend(unnamed);
        let found = parse_names(&run(Some(repo), &args)?, kind);
        for (hash, name) in &mut names {
            if name.is_none() {
                *name = found
                    .iter()
                    .find(|(h, _)| h == hash)
                    .and_then(|(_, n)| n.clone());
            }
        }
    }
    Ok(names)
}

/// Which refs a `name-rev` pass named from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefKind {
    Branch,
    Remote,
    Tag,
}

/// `name-rev`'s `<hash> <name>~2` lines from a pass over `kind` refs: the ref of each name,
/// read for a person.
fn parse_names(out: &str, kind: RefKind) -> Vec<(String, Option<String>)> {
    out.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(' ')?;
            Some((hash.to_owned(), ref_label(name, kind)))
        })
        .collect()
}

/// `remotes/origin/main~2^2` as `origin/main`, `tags/v1^0` as `tag v1`, `main` as itself;
/// `undefined` (no ref has it) as `None`. A ref name has no `~` nor `^`. Which kind it is comes
/// from the pass, not the name: a branch can be called `tags/x` or `origin/main`, and git
/// prints it so, without `heads/`.
fn ref_label(name: &str, kind: RefKind) -> Option<String> {
    let name = name.split(['~', '^']).next().unwrap_or(name).trim();
    if name.is_empty() || name == "undefined" {
        return None;
    }
    let under = |prefix: &str| name.strip_prefix(prefix).unwrap_or(name).to_owned();
    Some(match kind {
        RefKind::Branch => under("heads/"),
        RefKind::Remote => under("remotes/"),
        RefKind::Tag => format!("tag {}", under("tags/")),
    })
}

/// How many commits of `range` `upstream` has, when any: `None` when it has none, or is gone.
fn pushed(repo: &Path, range: &Range, upstream: &str) -> Result<Option<Pushed>, String> {
    if super::resolve(repo, upstream).is_err() {
        return Ok(None);
    }
    let total = count(repo, &[], &range.revisions())?;
    let mut not_pushed = range.revisions();
    not_pushed.push(format!("^{upstream}"));
    let pushed = total.saturating_sub(count(repo, &[], &not_pushed)?);
    Ok((pushed > 0).then(|| Pushed {
        upstream: upstream
            .strip_prefix("refs/remotes/")
            .unwrap_or(upstream)
            .to_owned(),
        commits: pushed,
    }))
}

/// The uncommitted files `files` loses, sorted.
fn lost_files(repo: &Path, files: &LostFiles) -> Result<Vec<String>, String> {
    let mut paths = match files {
        LostFiles::None => return Ok(Vec::new()),
        LostFiles::Listed(paths) => paths.clone(),
        LostFiles::HardReset(to) => {
            // Not what is in submodules: a reset leaves their working trees as they are.
            let out = run(
                Some(repo),
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=no",
                    "--ignore-submodules=all",
                ],
            )?;
            let status = parse_status(&out);
            let mut paths: Vec<String> = (status.staged.into_iter())
                .chain(status.unstaged)
                .chain(status.conflicted)
                .map(|f| f.path)
                .collect();
            if let Some(to) = to {
                paths.extend(written_over(repo, to)?);
            }
            paths
        }
    };
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

/// The untracked or ignored files `reset --hard <to>` writes over: those at a path `to` has a
/// file at and HEAD hasn't (one HEAD has is tracked, and counted as a change).
fn written_over(repo: &Path, to: &str) -> Result<Vec<String>, String> {
    let added = run(
        Some(repo),
        &[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-only",
            "-z",
            "--diff-filter=A",
            "HEAD",
            END_OPTS,
            to,
            "--",
        ],
    );
    // `--end-of-options` there is git 2.30+: older git reads `to` the same without it.
    let added = match added {
        Ok(out) => out,
        Err(_) => run(
            Some(repo),
            &[
                "diff",
                "--no-ext-diff",
                "--no-renames",
                "--name-only",
                "-z",
                "--diff-filter=A",
                "HEAD",
                super::not_an_option(to)?,
                "--",
            ],
        )?,
    };
    let added: Vec<&str> = added.split('\0').filter(|p| !p.is_empty()).collect();
    if added.is_empty() {
        return Ok(Vec::new());
    }
    // Every file not tracked, ignored ones too; a folder git tracks nothing in as one entry.
    let untracked = run(Some(repo), &["ls-files", "-z", "-o", "--directory"])?;
    let untracked: Vec<&str> = untracked.split('\0').filter(|p| !p.is_empty()).collect();
    Ok(added
        .into_iter()
        .filter(|path| {
            untracked
                .iter()
                .any(|u| u == path || (u.ends_with('/') && path.starts_with(u)))
        })
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_labels_read_for_a_person() {
        assert_eq!(
            ref_label("main~2", RefKind::Branch),
            Some("main".to_owned())
        );
        assert_eq!(
            ref_label("remotes/origin/feat/x^2~1", RefKind::Remote),
            Some("origin/feat/x".to_owned())
        );
        assert_eq!(
            ref_label("tags/v1.0^0", RefKind::Tag),
            Some("tag v1.0".to_owned())
        );
        assert_eq!(
            ref_label("tags/x~1", RefKind::Branch),
            Some("tags/x".to_owned())
        );
        assert_eq!(ref_label("undefined", RefKind::Tag), None);
        assert_eq!(ref_label("", RefKind::Branch), None);
    }

    #[test]
    fn other_refs_leave_out_only_the_moving_ones() {
        let moving = [
            "refs/heads/feat/x".to_owned(),
            "refs/remotes/origin/feat/x".to_owned(),
            "HEAD".to_owned(),
        ];
        assert_eq!(
            other_refs(&moving),
            [
                "--not",
                "--exclude=feat/x",
                "--branches",
                "--exclude=origin/feat/x",
                "--exclude=*/HEAD",
                "--remotes",
                "--tags",
                "--not",
            ]
        );
    }

    #[test]
    fn a_range_without_keep_is_the_tip_alone() {
        assert_eq!(Range::new("abc", Some("")).revisions(), ["abc"]);
        assert_eq!(Range::new("abc", None).revisions(), ["abc"]);
        assert_eq!(Range::new("abc", Some("def")).revisions(), ["abc", "^def"]);
    }

    #[test]
    fn name_rev_lines_parse() {
        let out = "aaa main~1\nbbb undefined\nccc remotes/origin/main\n";
        assert_eq!(
            parse_names(out, RefKind::Remote),
            [
                ("aaa".to_owned(), Some("main".to_owned())),
                ("bbb".to_owned(), None),
                ("ccc".to_owned(), Some("origin/main".to_owned())),
            ]
        );
    }

    #[test]
    fn asks_name_what_moves() {
        let reset = PreviewAsk::reset("HEAD", "h", "t", None, false);
        assert_eq!(reset.moving, ["HEAD"]);
        assert_eq!(reset.files, LostFiles::None);
        let hard = PreviewAsk::reset(
            "main",
            "h",
            "t",
            Some("refs/remotes/origin/main".into()),
            true,
        );
        assert_eq!(hard.moving, ["refs/heads/main"]);
        assert_eq!(hard.files, LostFiles::HardReset(Some("t".to_owned())));
        assert_eq!(hard.rewritten, Some(Range::new("h", Some("t"))));
        // HEAD unborn: everything on the branch leaves.
        let delete = PreviewAsk::delete(&["tip"], "", vec!["refs/heads/x".into()]);
        assert_eq!(delete.leaving, Some(Range::new("tip", None)));
        let dropped = PreviewAsk::drop_commit("c", "p", "main", "h", None);
        assert_eq!(dropped.leaving, Some(Range::new("c", Some("p"))));
        assert_eq!(dropped.rewritten, Some(Range::new("h", Some("p"))));
    }

    #[test]
    fn only_git_work_reads_git() {
        let mut ask = PreviewAsk::nothing();
        assert!(!ask.reads_git());
        ask.files = LostFiles::Listed(vec!["a".to_owned()]);
        assert!(!ask.reads_git());
        ask.files = LostFiles::HardReset(None);
        assert!(ask.reads_git());
    }
}
