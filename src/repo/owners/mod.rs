//! CODEOWNERS in the tab: who owns each file of a list (on its row), who a change needs (above
//! the list: "Needs payments and infra"), the reviewers the _Create pull request_ dialog
//! suggests, and the check of the file itself (patterns matching nothing, files without an
//! owner). The file is read at HEAD in the background, again when HEAD or the forge changes.

mod lint;
mod suggest;
#[cfg(test)]
mod tests;
mod view;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use egui::Context;

use super::RepoTab;
use crate::codeowners::{self, CodeOwners};
use crate::forge::Forge;

pub(crate) use lint::{LintView, OwnersAct};
pub(crate) use suggest::{Suggested, asked, reviewers_ui};

/// How many lists' summaries are kept.
const SUMMARIES: usize = 32;

/// What the tab knows of the repository's CODEOWNERS.
#[derive(Default)]
pub(crate) struct Owners {
    /// The file at HEAD, if there is one.
    file: Option<Arc<CodeOwners>>,
    /// Why it couldn't be read last time (what was read before stays).
    error: Option<String>,
    /// The HEAD and forge it was read for: read again when either changes.
    read_for: Option<(String, Option<Forge>)>,
    reading: bool,
    /// The owners of each file asked about, worked out once per file read. Only the UI thread
    /// uses it.
    cache: RefCell<HashMap<String, Arc<Vec<String>>>>,
    /// The summaries of the lists of files shown, by a hash of their paths: `None` while being
    /// worked out in the background.
    summaries: RefCell<HashMap<u64, Option<Arc<Summary>>>>,
    /// The reviewers of the _Create pull request_ dialog.
    suggest: suggest::Suggest,
    /// The check of the file, while its window is open.
    pub(crate) lint: Option<LintView>,
    /// Numbers the checks, so that one still running when the window is opened again isn't
    /// taken for the new one.
    lint_requests: u64,
}

/// Who a list of files needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub owners: Vec<String>,
    pub unowned: usize,
    pub files: usize,
}

impl Owners {
    /// The file read, if any.
    pub(crate) fn file(&self) -> Option<&CodeOwners> {
        self.file.as_deref()
    }

    /// The owners of the file at `path`.
    pub(crate) fn of(&self, path: &str) -> Arc<Vec<String>> {
        let Some(file) = &self.file else {
            return Arc::default();
        };
        if let Some(found) = self.cache.borrow().get(path) {
            return found.clone();
        }
        let owners = Arc::new(file.owners(path));
        self.cache
            .borrow_mut()
            .insert(path.to_owned(), owners.clone());
        owners
    }

    fn take(&mut self, file: Option<CodeOwners>) {
        let same = match (&self.file, &file) {
            (Some(a), Some(b)) => a.blob == b.blob && a.path == b.path && a.flavor == b.flavor,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.file = file.map(Arc::new);
            self.cache.borrow_mut().clear();
            self.summaries.borrow_mut().clear();
            // What was suggested came from the file before.
            self.suggest.forget();
        }
    }
}

impl RepoTab {
    /// Each frame: reads the file again when HEAD or the forge changed, and suggests reviewers
    /// in the _Create pull request_ dialog once its branch and base are known.
    pub(super) fn owners_frame(&mut self, ctx: &Context) {
        let key = (self.refs.head.clone(), self.forge());
        if self.history_gen > 0
            && !self.owners.reading
            && self.owners.read_for.as_ref() != Some(&key)
        {
            self.read_owners(key, ctx);
        }
        self.suggest_reviewers(ctx);
    }

    fn read_owners(&mut self, key: (String, Option<Forge>), ctx: &Context) {
        self.owners.reading = true;
        let repo = self.path.clone();
        let forge = key.1;
        let known = self.owners.file.clone();
        self.then(
            ctx,
            move || {
                std::panic::catch_unwind(|| {
                    codeowners::read(&repo, "HEAD", forge, known.as_deref())
                })
                .unwrap_or_else(|_| Err("reading it failed".to_owned()))
            },
            move |tab, result, _| {
                tab.owners.reading = false;
                tab.owners.read_for = Some(key);
                match result {
                    Ok(file) => {
                        tab.owners.error = None;
                        tab.owners.take(file);
                    }
                    // A read that failed (a connection lost) leaves what was read before.
                    Err(e) => tab.owners.error = Some(e),
                }
            },
        );
    }

    /// Who the files at `paths` need: worked out in the background the first time a list is
    /// shown, then kept (`None` meanwhile).
    pub(super) fn owners_summary_of(&self, paths: &[&str], ctx: &Context) -> Option<Arc<Summary>> {
        let file = self.owners.file.clone()?;
        if paths.is_empty() {
            return None;
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            paths.hash(&mut h);
            file.blob.hash(&mut h);
            h.finish()
        };
        let mut summaries = self.owners.summaries.borrow_mut();
        if let Some(found) = summaries.get(&key) {
            return found.clone();
        }
        if summaries.len() >= SUMMARIES {
            summaries.clear();
        }
        summaries.insert(key, None);
        drop(summaries);
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        let blob = file.blob.clone();
        self.then(
            ctx,
            move || {
                let (owners, unowned) =
                    codeowners::owners_of(&file, paths.iter().map(String::as_str));
                Summary {
                    owners,
                    unowned,
                    files: paths.len(),
                }
            },
            move |tab, summary, _| {
                // Still the same file: its answers hold.
                if tab.owners.file.as_ref().is_some_and(|f| f.blob == blob) {
                    tab.owners
                        .summaries
                        .borrow_mut()
                        .insert(key, Some(Arc::new(summary)));
                }
            },
        );
        None
    }
}
