//! What a branch was before gitcraft rewrote it (a rebase, an amend, a quick edit…) and what it
//! is after, kept in refs under `refs/gitcraft/rewrites/` so that the old commits stay (`git gc`
//! keeps what a ref reaches) and the two versions can be compared later. Old ones are pruned.
//!
//! `refs/gitcraft/rewrites/<branch>/<time>/{old-base,old-tip,new-base,new-tip}`, `<branch>` the
//! branch's name (`HEAD` for a detached HEAD, which no branch can be called) and `<time>` when
//! it happened, in Unix seconds.

use std::path::Path;

use super::range_diff::Series;
use super::{END_OPTS, FS, run};

const ROOT: &str = "refs/gitcraft/rewrites/";
const ROLES: [&str; 4] = ["old-base", "old-tip", "new-base", "new-tip"];

/// Rewrites older than this are pruned.
pub const KEEP_SECS: i64 = 30 * 24 * 60 * 60;
/// And no more than this many of each branch are kept, the newest.
pub const KEEP_PER_BRANCH: usize = 20;

/// One rewrite of a branch: its commits before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewrite {
    /// The branch's name, `HEAD` when detached.
    pub branch: String,
    /// When, in Unix seconds.
    pub time: i64,
    pub old: Series,
    pub new: Series,
}

impl Rewrite {
    /// The ref of one of its ends (`old-tip`…).
    fn ref_name(&self, role: &str) -> String {
        format!("{ROOT}{}/{}/{role}", self.branch, self.time)
    }
}

/// The commits a rewrite of the branch replaced and those it made: `None` when `before` is
/// part of `after` (a commit, a merge, a fast-forward: nothing was rewritten), or the two have
/// no history in common. `onto`, when the operation rebased onto something (a branch,
/// `@{upstream}`), is where the new series starts and where the old one forked from it.
pub fn rewritten(
    repo: &Path,
    before: &str,
    after: &str,
    onto: Option<&str>,
) -> Result<Option<(Series, Series)>, String> {
    if before.is_empty() || after.is_empty() || before == after {
        return Ok(None);
    }
    let before = super::resolve(repo, before)?;
    let after = super::resolve(repo, after)?;
    if is_ancestor(repo, &before, &after) {
        return Ok(None);
    }
    let onto = onto.and_then(|o| super::resolve(repo, o).ok());
    let bases = match onto {
        Some(onto) => (
            super::merge_base(repo, &before, &onto),
            super::merge_base(repo, &after, &onto),
        ),
        None => {
            let base = super::merge_base(repo, &before, &after);
            (base.clone(), base)
        }
    };
    let (Ok(old_base), Ok(new_base)) = bases else {
        return Ok(None);
    };
    Ok(Some((
        Series::new(&old_base, &before),
        Series::new(&new_base, &after),
    )))
}

/// Whether `a` is `b` or an ancestor of it.
pub fn is_ancestor(repo: &Path, a: &str, b: &str) -> bool {
    run(Some(repo), &["merge-base", "--is-ancestor", END_OPTS, a, b]).is_ok()
}

/// Keeps the two versions of `branch` (`refs/heads/x` or `x`; empty for a detached HEAD) as a
/// rewrite made at `time`.
pub fn record_rewrite(
    repo: &Path,
    branch: &str,
    old: &Series,
    new: &Series,
    time: i64,
) -> Result<Rewrite, String> {
    let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
    let branch = if branch.is_empty() { "HEAD" } else { branch };
    // Two rewrites in the same second each get their own.
    let taken: Vec<i64> = rewrites(repo)?
        .into_iter()
        .filter(|r| r.branch == branch)
        .map(|r| r.time)
        .collect();
    let mut time = time;
    while taken.contains(&time) {
        time += 1;
    }
    let rewrite = Rewrite {
        branch: branch.to_owned(),
        time,
        old: old.clone(),
        new: new.clone(),
    };
    let targets = [&old.base, &old.tip, &new.base, &new.tip];
    for (role, target) in ROLES.iter().zip(targets) {
        // `update-ref <ref> <new> ""`: made, never moved over one that is there already.
        run(
            Some(repo),
            &[
                "update-ref",
                "-m",
                "gitcraft: before / after a rewrite",
                &rewrite.ref_name(role),
                target,
                "",
            ],
        )?;
    }
    Ok(rewrite)
}

/// Every rewrite kept, newest first.
pub fn rewrites(repo: &Path) -> Result<Vec<Rewrite>, String> {
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            &format!("--format=%(refname){FS}%(objectname)"),
            ROOT,
        ],
    )?;
    Ok(parse_rewrites(&out))
}

/// `for-each-ref`'s lines grouped into rewrites; one that misses an end is left out.
fn parse_rewrites(out: &str) -> Vec<Rewrite> {
    let mut found: Vec<(String, i64, [Option<String>; 4])> = Vec::new();
    for line in out.lines() {
        let Some((name, oid)) = line.split_once(FS) else {
            continue;
        };
        let Some(rest) = name.strip_prefix(ROOT) else {
            continue;
        };
        let mut parts = rest.rsplitn(3, '/');
        let (Some(role), Some(time), Some(branch)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Some(slot), Ok(time)) = (ROLES.iter().position(|r| *r == role), time.parse()) else {
            continue;
        };
        let at = match found.iter().position(|(b, t, _)| b == branch && *t == time) {
            Some(at) => at,
            None => {
                found.push((branch.to_owned(), time, Default::default()));
                found.len() - 1
            }
        };
        found[at].2[slot] = Some(oid.to_owned());
    }
    let mut list: Vec<Rewrite> = found
        .into_iter()
        .filter_map(|(branch, time, ends)| {
            let [Some(old_base), Some(old_tip), Some(new_base), Some(new_tip)] = ends else {
                return None;
            };
            Some(Rewrite {
                branch,
                time,
                old: Series::new(&old_base, &old_tip),
                new: Series::new(&new_base, &new_tip),
            })
        })
        .collect();
    list.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.branch.cmp(&b.branch)));
    list
}

/// The rewrites to let go of at `now`: older than [`KEEP_SECS`], or past the newest
/// [`KEEP_PER_BRANCH`] of their branch. `list` newest first, as [`rewrites`] gives it.
pub fn to_prune(list: &[Rewrite], now: i64) -> Vec<&Rewrite> {
    let mut per_branch: std::collections::HashMap<&str, usize> = Default::default();
    list.iter()
        .filter(|r| {
            let n = per_branch.entry(r.branch.as_str()).or_default();
            *n += 1;
            *n > KEEP_PER_BRANCH || now.saturating_sub(r.time) > KEEP_SECS
        })
        .collect()
}

/// Deletes the rewrites [`to_prune`] picks; says how many.
pub fn prune_rewrites(repo: &Path, now: i64) -> Result<usize, String> {
    let list = rewrites(repo)?;
    let old = to_prune(&list, now);
    for rewrite in &old {
        delete_rewrite(repo, rewrite)?;
    }
    Ok(old.len())
}

/// Lets go of the refs of `rewrite` (the commits only it kept can then be collected).
pub fn delete_rewrite(repo: &Path, rewrite: &Rewrite) -> Result<(), String> {
    for role in ROLES {
        // Gone already (another window pruned it): what was asked is done.
        let name = rewrite.ref_name(role);
        if run(Some(repo), &["update-ref", "-d", &name]).is_err()
            && super::resolve(repo, &name).is_ok()
        {
            return Err(format!("could not delete {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
