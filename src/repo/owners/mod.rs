//! CODEOWNERS in the tab: who owns each file of a list (on its row), who a change needs (above
//! the list: "Needs payments and infra"), the reviewers the _Create pull request_ dialog
//! suggests, and the check of the file itself (patterns matching nothing, files without an
//! owner). The file is read at HEAD, in the background, again when it changes.

mod lint;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Context, RichText, Ui};

use super::{Dialog, RepoTab};
use crate::codeowners::{self, CodeOwners, Reviewer};
use crate::repo::widgets::OwnerTag;
use crate::theme::Tokens;

pub(crate) use lint::{LintView, OwnersAct};

/// What the tab knows of the repository's CODEOWNERS.
#[derive(Default)]
pub(crate) struct Owners {
    /// The file at HEAD, if there is one.
    file: Option<Arc<CodeOwners>>,
    /// Why it couldn't be read.
    error: Option<String>,
    /// The read (`history_gen`) it was read for.
    read_for: u64,
    reading: bool,
    /// The owners of each file asked about, worked out once per file read.
    cache: Mutex<HashMap<String, Arc<Vec<String>>>>,
    /// The branch and base the _Create pull request_ dialog's reviewers were asked for.
    suggested_for: Option<(String, String)>,
    /// The check of the file, while its window is open.
    pub(crate) lint: Option<LintView>,
}

/// One reviewer the _Create pull request_ dialog suggests, and whether it is ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggested {
    pub reviewer: Reviewer,
    pub ask: bool,
}

/// The reviewers the dialog asks for: those ticked, as the forges take them (`ann`,
/// `acme/payments`).
pub(crate) fn asked(suggested: &[Suggested]) -> Vec<String> {
    suggested
        .iter()
        .filter(|s| s.ask)
        .map(|s| s.reviewer.name.clone())
        .collect()
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
        let mut cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(poisoned) => poisoned.into_inner(),
        };
        cache
            .entry(path.to_owned())
            .or_insert_with(|| Arc::new(file.owners(path)))
            .clone()
    }

    /// What the row of the file at `path` shows of its owners.
    pub(crate) fn tag(&self, path: &str) -> Option<OwnerTag> {
        let owners = self.of(path);
        let first = owners.first()?;
        let label = match owners.len() {
            1 => codeowners::short_owner(first).to_owned(),
            n => format!("{} +{}", codeowners::short_owner(first), n - 1),
        };
        Some(OwnerTag {
            label,
            full: owners.join(", "),
        })
    }

    /// Every owner of `paths`, and how many of them have none.
    pub(crate) fn needed<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a str>,
    ) -> (Vec<String>, usize) {
        let mut all: Vec<String> = Vec::new();
        let mut unowned = 0;
        for path in paths {
            let owners = self.of(path);
            if owners.is_empty() {
                unowned += 1;
            }
            for owner in owners.iter() {
                if !all.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
                    all.push(owner.clone());
                }
            }
        }
        (all, unowned)
    }

    fn take(&mut self, file: Option<CodeOwners>) {
        let same = match (&self.file, &file) {
            (Some(a), Some(b)) => a.blob == b.blob && a.path == b.path,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.file = file.map(Arc::new);
            if let Ok(mut cache) = self.cache.lock() {
                cache.clear();
            }
        }
    }
}

/// "Needs payments and infra · 2 files without an owner", for the change of a list's files.
pub(crate) fn summary_text(owners: &[String], unowned: usize, files: usize) -> Option<String> {
    if owners.is_empty() && unowned == 0 {
        return None;
    }
    let names: Vec<String> = owners
        .iter()
        .map(|o| codeowners::short_owner(o).to_owned())
        .collect();
    let mut text = if names.is_empty() {
        String::new()
    } else {
        format!("Needs {}", codeowners::and_list(&names))
    };
    if unowned > 0 {
        if !text.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(&if unowned == files {
            if files == 1 {
                "No owner".to_owned()
            } else {
                "None of them has an owner".to_owned()
            }
        } else if unowned == 1 {
            "1 file without an owner".to_owned()
        } else {
            format!("{unowned} files without an owner")
        });
    }
    Some(text)
}

impl RepoTab {
    /// Each frame: reads the file again after a reload, and suggests reviewers in the _Create
    /// pull request_ dialog once its branch and base are known.
    pub(super) fn owners_frame(&mut self, ctx: &Context) {
        if self.owners.read_for != self.history_gen && !self.owners.reading && self.history_gen > 0
        {
            self.read_owners(ctx);
        }
        self.suggest_reviewers(ctx);
    }

    fn read_owners(&mut self, ctx: &Context) {
        self.owners.reading = true;
        let read_for = self.history_gen;
        let repo = self.path.clone();
        let forge = self.forge();
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
                tab.owners.read_for = read_for;
                match result {
                    Ok(file) => {
                        tab.owners.error = None;
                        tab.owners.take(file);
                    }
                    Err(e) => {
                        tab.owners.error = Some(e);
                        tab.owners.take(None);
                    }
                }
            },
        );
    }

    /// Fills the _Create pull request_ dialog's reviewers with the owners of what its branch
    /// changes since its base.
    fn suggest_reviewers(&mut self, ctx: &Context) {
        let Some(file) = self.owners.file.clone() else {
            return;
        };
        let Some(Dialog::CreatePr { branch, base, .. }) = &self.dialog else {
            self.owners.suggested_for = None;
            return;
        };
        let key = (branch.clone(), base.trim().to_owned());
        if key.1.is_empty() || self.owners.suggested_for.as_ref() == Some(&key) {
            return;
        }
        self.owners.suggested_for = Some(key.clone());
        let repo = self.path.clone();
        let remote = self.default_remote();
        self.then(
            ctx,
            move || {
                let files = changed_files(&repo, &key.0, &key.1, remote.as_deref())?;
                Ok::<_, String>(codeowners::reviewers_for(
                    &file,
                    files.iter().map(String::as_str),
                ))
            },
            move |tab, reviewers, _| {
                let Some(Dialog::CreatePr {
                    branch,
                    base,
                    reviewers: shown,
                    ..
                }) = &mut tab.dialog
                else {
                    return;
                };
                // Answered for the branch and base shown, and only then.
                if tab.owners.suggested_for.as_ref()
                    != Some(&(branch.clone(), base.trim().to_owned()))
                {
                    return;
                }
                let reviewers = reviewers.unwrap_or_default();
                let before = std::mem::take(shown);
                *shown = reviewers
                    .into_iter()
                    .map(|reviewer| {
                        // What was unticked stays unticked.
                        let ask = before
                            .iter()
                            .find(|s| s.reviewer == reviewer)
                            .is_none_or(|s| s.ask);
                        Suggested { reviewer, ask }
                    })
                    .collect();
            },
        );
    }

    /// What a row of the file at `path` shows of its owners.
    pub(super) fn owner_tag(&self, path: &str) -> Option<OwnerTag> {
        self.owners.tag(path)
    }

    /// The line above a list of files that says who they need.
    pub(super) fn owners_summary<'a>(&self, ui: &mut Ui, paths: impl IntoIterator<Item = &'a str>) {
        let Some(file) = self.owners.file() else {
            return;
        };
        let paths: Vec<&str> = paths.into_iter().collect();
        if paths.is_empty() {
            return;
        }
        let (owners, unowned) = self.owners.needed(paths.iter().copied());
        let Some(text) = summary_text(&owners, unowned, paths.len()) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 4))
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).color(t.text_dim).small()).truncate())
                    .on_hover_text(format!(
                        "From {} ({} rules){}",
                        file.path,
                        file.rule_count(),
                        if owners.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", owners.join(", "))
                        }
                    ));
            });
    }
}

/// The files `branch` changes since it left `base` (the remote's `base` when there is one, as
/// the pull request will be against it).
fn changed_files(
    repo: &std::path::Path,
    branch: &str,
    base: &str,
    remote: Option<&str>,
) -> Result<Vec<String>, String> {
    let candidates = remote
        .map(|r| format!("refs/remotes/{r}/{base}"))
        .into_iter()
        .chain([format!("refs/heads/{base}")]);
    let base = candidates
        .into_iter()
        .find(|r| crate::git::resolve(repo, r).is_ok())
        .ok_or_else(|| format!("{base} isn't a branch here"))?;
    let range = format!("{base}...refs/heads/{branch}");
    let out = crate::git::run(
        Some(repo),
        &["diff", "--name-only", "-z", "--no-renames", &range, "--"],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The reviewers part of the _Create pull request_ dialog: the owners of what it changes, ticked
/// to be asked.
pub(crate) fn reviewers_ui(ui: &mut Ui, suggested: &mut [Suggested]) {
    if suggested.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(
        RichText::new("Ask for reviews from (CODEOWNERS)")
            .color(t.text_dim)
            .small(),
    );
    ui.horizontal_wrapped(|ui| {
        for s in suggested.iter_mut() {
            let label = s.reviewer.label();
            let tip = if s.reviewer.team {
                "A team: the forge asks its members"
            } else {
                "An account"
            };
            ui.checkbox(&mut s.ask, label).on_hover_text(tip);
        }
    });
}
