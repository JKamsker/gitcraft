//! Stacked branches: the branch each one is based on (its parent), recorded in the
//! repository's own git config as `branch.<name>.gitcraft-parent`, with the parent's commit it
//! was last based on as `branch.<name>.gitcraft-parent-base` (what a restack replays from).
//! A layer of a stack is a branch with a parent; its parent may be a layer too
//! (`main → schema → api → ui`).

pub mod restack;
#[cfg(test)]
mod restack_tests;
mod suggest;
#[cfg(test)]
pub(crate) mod test_repo;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

pub use suggest::suggest_parent;

use super::{count_apart, merge_base, run_ok_codes};

/// The config key (under `branch.<name>`) naming the branch's parent.
pub const PARENT_KEY: &str = "gitcraft-parent";
/// The config key (under `branch.<name>`) holding the parent's commit the branch is based on.
pub const BASE_KEY: &str = "gitcraft-parent-base";

/// What the config says a branch is based on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    /// The parent branch (a local one).
    pub parent: String,
    /// The parent's commit the branch was last based on, when known.
    pub base: Option<String>,
}

/// Every branch → what it is recorded as based on. Branches without a parent are left out.
pub fn recorded(repo: &Path) -> Result<BTreeMap<String, Recorded>, String> {
    let pattern = format!(r"^branch\..*\.({PARENT_KEY}|{BASE_KEY})$");
    // Exit code 1: nothing matches.
    let out = run_ok_codes(
        Some(repo),
        &["config", "--local", "-z", "--get-regexp", &pattern],
        &[0, 1],
    )?;
    Ok(parse_recorded(&out))
}

/// What the config records ([`recorded`]) and the restack under way ([`restack::pending`]),
/// read in one go.
pub fn read_all(
    repo: &Path,
) -> Result<(BTreeMap<String, Recorded>, Option<restack::Pending>), String> {
    let pattern = format!(
        r"^(branch\..*\.({PARENT_KEY}|{BASE_KEY})|{}\..*)$",
        restack::SECTION
    );
    let out = run_ok_codes(
        Some(repo),
        &["config", "--local", "-z", "--get-regexp", &pattern],
        &[0, 1],
    )?;
    Ok((parse_recorded(&out), restack::parse_pending(&out)))
}

/// `git config -z --get-regexp` output (`key\nvalue\0`…) → branch → what it is based on.
pub(crate) fn parse_recorded(out: &str) -> BTreeMap<String, Recorded> {
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut bases: BTreeMap<String, String> = BTreeMap::new();
    for entry in out.split('\0').filter(|e| !e.is_empty()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        // The section and the key's name come lowercased; the branch keeps its case.
        let Some(rest) = key.strip_prefix("branch.") else {
            continue;
        };
        let Some((branch, name)) = rest.rsplit_once('.') else {
            continue;
        };
        let value = value.trim();
        if branch.is_empty() || value.is_empty() {
            continue;
        }
        let into = match name {
            PARENT_KEY => &mut parents,
            BASE_KEY => &mut bases,
            _ => continue,
        };
        // The last one set wins, as git reads it.
        into.insert(branch.to_owned(), value.to_owned());
    }
    parents
        .into_iter()
        .map(|(branch, parent)| {
            let base = bases.remove(&branch);
            (branch, Recorded { parent, base })
        })
        .collect()
}

/// `branch.<branch>.<key>` as git config takes it.
pub fn config_key(branch: &str, key: &str) -> String {
    format!("branch.{branch}.{key}")
}

/// The steps that record `branch` as based on `parent`, at the parent's commit `base`.
pub fn set_parent_ops(branch: &str, parent: &str, base: &str) -> Vec<Vec<String>> {
    vec![
        set_op(&config_key(branch, PARENT_KEY), parent),
        set_op(&config_key(branch, BASE_KEY), base),
    ]
}

/// The steps that take `branch` out of its stack: it has no parent any more.
pub fn unset_parent_ops(branch: &str) -> Vec<Vec<String>> {
    [PARENT_KEY, BASE_KEY]
        .iter()
        .flat_map(|key| unset_ops(&config_key(branch, key)))
        .collect()
}

/// The steps that put back what `branch` was recorded as based on before (`None`: nothing).
pub fn restore_ops(branch: &str, before: Option<&Recorded>) -> Vec<Vec<String>> {
    match before {
        None => unset_parent_ops(branch),
        Some(r) => {
            let mut steps = vec![set_op(&config_key(branch, PARENT_KEY), &r.parent)];
            let base_key = config_key(branch, BASE_KEY);
            match &r.base {
                Some(base) => steps.push(set_op(&base_key, base)),
                None => steps.extend(unset_ops(&base_key)),
            }
            steps
        }
    }
}

/// `git config --local --replace-all key value`: one value however many the key had (a plain
/// set refuses a key set more than once by hand).
pub fn set_op(key: &str, value: &str) -> Vec<String> {
    ["config", "--local", "--replace-all", key, value]
        .map(str::to_owned)
        .to_vec()
}

/// Unsets `key` whether it is set or not: `git config --unset` fails on a key that isn't
/// there, which would stop the steps after it, so it is set (to nothing) first.
pub(crate) fn unset_ops(key: &str) -> Vec<Vec<String>> {
    vec![
        set_op(key, ""),
        ["config", "--local", "--unset-all", key]
            .map(str::to_owned)
            .to_vec(),
    ]
}

/// Whether making `branch` based on `parent` would make a loop: the parent is the branch, or
/// is based (through any number of layers) on it.
pub fn would_loop(recorded: &BTreeMap<String, Recorded>, branch: &str, parent: &str) -> bool {
    let mut at = parent;
    let mut seen: Vec<&str> = Vec::new();
    loop {
        if at == branch {
            return true;
        }
        // A loop already in the config (edited by hand) that `branch` isn't part of.
        if seen.contains(&at) {
            return false;
        }
        seen.push(at);
        match recorded.get(at) {
            Some(r) => at = &r.parent,
            None => return false,
        }
    }
}

/// The branches recorded as based on `parent`.
pub fn children(recorded: &BTreeMap<String, Recorded>, parent: &str) -> Vec<String> {
    recorded
        .iter()
        .filter(|(_, r)| r.parent == parent)
        .map(|(b, _)| b.clone())
        .collect()
}

/// The steps that make the layers on `old` layers on `new` instead: what renaming `old` needs.
pub fn reparent_ops(children: &[String], new: &str) -> Vec<Vec<String>> {
    children
        .iter()
        .map(|child| set_op(&config_key(child, PARENT_KEY), new))
        .collect()
}

/// One layer of a stack as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub branch: String,
    pub parent: String,
    /// The parent's commit it was last based on, as recorded.
    pub base: Option<String>,
    /// Commits of its own: on the branch and not on its parent.
    pub own: u32,
    /// Commits the parent has that the branch hasn't: when there are any, the branch is no
    /// longer on its parent's tip and needs restacking.
    pub behind: u32,
    /// The parent branch is gone (deleted or renamed).
    pub parent_missing: bool,
}

impl Layer {
    /// The parent moved on (amended, rebased, got commits) since the branch was based on it.
    pub fn needs_restack(&self) -> bool {
        !self.parent_missing && self.behind > 0
    }
}

/// The layers of every stack: each branch of `local` with a recorded parent, how many commits
/// it has of its own and whether its parent moved on. A recorded branch that is gone is left
/// out.
pub fn layers(
    repo: &Path,
    recorded: &BTreeMap<String, Recorded>,
    local: &[String],
) -> Result<Vec<Layer>, String> {
    let present: Vec<(&String, &Recorded)> = recorded
        .iter()
        .filter(|(branch, _)| local.contains(branch))
        .collect();
    // Side by side: on another machine each count is a round trip.
    let counted = in_parallel(&present, |(branch, r)| {
        let parent_missing = !local.contains(&r.parent);
        let (behind, own) = if parent_missing {
            // Counted from the commit it was based on, if that is still there.
            r.base
                .as_deref()
                .and_then(|base| count_apart(repo, base, &local_ref(branch)).ok())
                .unwrap_or((0, 0))
        } else {
            count_apart(repo, &local_ref(&r.parent), &local_ref(branch))?
        };
        Ok(Layer {
            branch: (*branch).clone(),
            parent: r.parent.clone(),
            base: r.base.clone(),
            own,
            behind: if parent_missing { 0 } else { behind },
            parent_missing,
        })
    });
    counted.into_iter().collect()
}

/// How many git commands [`in_parallel`] runs at once.
const PARALLEL: usize = 8;

/// `work` on each of `items`, a few at a time on threads of their own, the answers in order.
pub(crate) fn in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let mut out = Vec::with_capacity(items.len());
    for chunk in items.chunks(PARALLEL) {
        std::thread::scope(|scope| {
            let running: Vec<_> = chunk
                .iter()
                .map(|item| scope.spawn(crate::connections::inherit(|| work(item))))
                .collect();
            for (item, job) in chunk.iter().zip(running) {
                // A job that panicked is done again here, where the panic shows.
                out.push(job.join().unwrap_or_else(|_| work(item)));
            }
        });
    }
    out
}

/// The commit `branch` forks from `parent` at: what it is based on when it is made a layer.
pub fn fork_point(repo: &Path, branch: &str, parent: &str) -> Result<String, String> {
    merge_base(repo, &local_ref(parent), &local_ref(branch)).map_err(|_| {
        format!("{branch} and {parent} have no commit in common: {branch} can't be based on it")
    })
}

/// `refs/heads/<name>`: never read as a tag or a remote branch of the same name.
pub fn local_ref(name: &str) -> String {
    format!("refs/heads/{name}")
}
