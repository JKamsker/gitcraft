//! Files better read folded: generated ones (`linguist-generated`, `-diff` in
//! `.gitattributes`), lockfiles, minified files and the patterns chosen in the settings. They
//! start folded in file lists and diffs, behind a line like _+1,204 −880, lockfile_.

mod banner;
mod list;
mod rules;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

pub(super) use banner::folded_diff;
pub(super) use list::split;
pub(super) use rules::{FoldReason, Rules, stats_line};

use crate::git::{FoldAttr, WorkStatus};

/// What a tab knows of its files' attributes, asked of git in the background.
#[derive(Default)]
pub(crate) struct Folding {
    /// What the attributes say, by path.
    attrs: HashMap<String, FoldAttr>,
    /// Paths asked about whose answer hasn't come yet.
    asked: HashSet<String>,
    /// Changes each time answers come in or are forgotten: lists work out again what they fold.
    pub(super) generation: u64,
    /// Changes each time the attributes are forgotten: answers asked before are dropped.
    epoch: u64,
    /// What a `.gitattributes` looked like at the last status read (path and status letter of
    /// those changed) and the commit checked out: when either changes, the attributes may have.
    seen: (Vec<(String, char)>, String),
}

impl Folding {
    /// Why `path` is folded, if it is (`rules`: the settings'); `None` also while its
    /// attributes aren't known yet, when only its name decides.
    pub(super) fn reason(&self, rules: &Rules, path: &str) -> Option<FoldReason> {
        rules.reason(path, self.attrs.get(path).copied())
    }

    /// Of `paths`, those whose attributes are neither known nor being asked about.
    pub(super) fn unknown<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut seen = HashSet::new();
        paths
            .into_iter()
            .filter(|p| !self.attrs.contains_key(*p) && !self.asked.contains(*p))
            .filter(|p| seen.insert(*p))
            .map(str::to_owned)
            .collect()
    }

    /// `paths` are being asked about: not again until the answer comes. Returns what the
    /// answer is to be handed back with, to tell it from one asked before the attributes were
    /// forgotten.
    pub(super) fn asking(&mut self, paths: &[String]) -> u64 {
        self.asked.extend(paths.iter().cloned());
        self.epoch
    }

    /// The answer about `paths`, asked at `epoch`. When git couldn't say (the repository is
    /// gone…), they count as saying nothing, so only their names decide and they aren't asked
    /// again. An answer asked before the attributes were forgotten is dropped: they may have
    /// changed since.
    pub(super) fn arrived(
        &mut self,
        epoch: u64,
        paths: Vec<String>,
        result: Result<HashMap<String, FoldAttr>, String>,
    ) {
        if epoch != self.epoch {
            return;
        }
        let mut found = result.unwrap_or_default();
        for path in paths {
            self.asked.remove(&path);
            let attr = found.remove(&path).unwrap_or(FoldAttr::Unspecified);
            self.attrs.insert(path, attr);
        }
        self.generation += 1;
    }

    /// After a status read: forgets the attributes when a `.gitattributes` changed (or is
    /// changed, and may have been edited again) or another commit is checked out.
    pub(super) fn saw_status(&mut self, work: &WorkStatus, head: &str) {
        let changed: Vec<(String, char)> = work
            .staged
            .iter()
            .chain(&work.unstaged)
            .chain(&work.conflicted)
            .filter(|f| is_attributes_file(&f.path))
            .map(|f| (f.path.clone(), f.code))
            .collect();
        let seen = (changed, head.to_owned());
        if seen != self.seen || !seen.0.is_empty() {
            self.seen = seen;
            self.forget();
        }
    }

    /// Forgets every attribute read: they are asked again as lists show their files.
    fn forget(&mut self) {
        if self.attrs.is_empty() && self.asked.is_empty() {
            return;
        }
        self.attrs.clear();
        self.asked.clear();
        self.epoch += 1;
        self.generation += 1;
    }
}

/// Whether `path` is a `.gitattributes` file, at the top or in a folder.
fn is_attributes_file(path: &str) -> bool {
    path == ".gitattributes" || path.ends_with("/.gitattributes")
}

impl super::RepoTab {
    /// Asks git, in the background, what `.gitattributes` says of those of `paths` not known
    /// yet; lists and diffs fold them once the answer is in.
    pub(super) fn ask_fold_attributes(&mut self, paths: &[String], ctx: &egui::Context) {
        let paths = self.folding.unknown(paths.iter().map(String::as_str));
        if paths.is_empty() {
            return;
        }
        let epoch = self.folding.asking(&paths);
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let found = crate::git::fold_attributes(&repo, &paths);
                (paths, found)
            },
            move |tab, (paths, found), _| tab.folding.arrived(epoch, paths, found),
        );
    }
}
