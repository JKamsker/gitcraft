//! The reviewers the _Create pull request_ dialog suggests: the owners of what the branch
//! changes since its base, read from the base's CODEOWNERS, as the forge reads it.

use std::collections::HashSet;

use egui::{Context, RichText, Ui};

use crate::codeowners::{self, Reviewer};
use crate::forge::Forge;
use crate::repo::{Dialog, RepoTab};
use crate::theme::Tokens;

/// One reviewer the dialog suggests, and whether it is ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggested {
    pub reviewer: Reviewer,
    pub ask: bool,
}

/// The reviewers the dialog asks for: those ticked, as the forges take them (`ann`,
/// `acme/payments`). A draft asks nobody: the forges leave drafts alone too.
pub(crate) fn asked(suggested: &[Suggested]) -> Vec<String> {
    suggested
        .iter()
        .filter(|s| s.ask)
        .map(|s| s.reviewer.name.clone())
        .collect()
}

/// What the suggestion keeps while the dialog is open.
#[derive(Default)]
pub(super) struct Suggest {
    /// The branch and base last asked about.
    asked_for: Option<(String, String)>,
    /// The reviewers unticked in the dialog, kept when the list is worked out again.
    unticked: HashSet<String>,
}

impl Suggest {
    /// Asks again on the next frame (the file changed).
    pub(super) fn forget(&mut self) {
        self.asked_for = None;
    }
}

impl RepoTab {
    /// Fills the _Create pull request_ dialog's reviewers with the owners of what its branch
    /// changes since its base.
    pub(super) fn suggest_reviewers(&mut self, ctx: &Context) {
        let Some(Dialog::CreatePr {
            branch,
            base,
            reviewers,
            ..
        }) = &self.dialog
        else {
            self.owners.suggest = Default::default();
            return;
        };
        // What the user unticked stays so.
        for s in reviewers.iter().filter(|s| !s.ask) {
            self.owners.suggest.unticked.insert(s.reviewer.name.clone());
        }
        for s in reviewers.iter().filter(|s| s.ask) {
            self.owners.suggest.unticked.remove(&s.reviewer.name);
        }
        let key = (branch.clone(), base.trim().to_owned());
        if key.1.is_empty() || self.owners.suggest.asked_for.as_ref() == Some(&key) {
            return;
        }
        self.owners.suggest.asked_for = Some(key.clone());
        let repo = self.path.clone();
        let remote = self.default_remote();
        let forge = self.forge();
        self.then(
            ctx,
            move || {
                reviewers_for(&repo, &key.0, &key.1, remote.as_deref(), forge).map(|r| (key, r))
            },
            move |tab, found, _| {
                // A base being typed doesn't resolve yet: what was suggested stays.
                let Ok((key, found)) = found else { return };
                let Some(Dialog::CreatePr {
                    branch,
                    base,
                    reviewers,
                    ..
                }) = &mut tab.dialog
                else {
                    return;
                };
                // Answered for the branch and base shown, and only then: an older answer
                // coming last is dropped.
                if (branch.as_str(), base.trim()) != (key.0.as_str(), key.1.as_str()) {
                    return;
                }
                let unticked = &tab.owners.suggest.unticked;
                *reviewers = found
                    .into_iter()
                    .map(|reviewer| Suggested {
                        ask: !unticked.contains(&reviewer.name),
                        reviewer,
                    })
                    .collect();
            },
        );
    }
}

/// The reviewers to suggest for `branch` against `base` (the remote's `base` when there is one,
/// as the pull request will be against it): the owners its changes have in the base's
/// CODEOWNERS, those the forge can be asked for.
fn reviewers_for(
    repo: &std::path::Path,
    branch: &str,
    base: &str,
    remote: Option<&str>,
    forge: Option<Forge>,
) -> Result<Vec<Reviewer>, String> {
    let base_ref = remote
        .map(|r| format!("refs/remotes/{r}/{base}"))
        .into_iter()
        .chain([format!("refs/heads/{base}")])
        .find(|r| crate::git::resolve(repo, r).is_ok())
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-owners-reviewers-for-d68c1cfb",
                &[("base", (base).into())],
            )
        })?;
    let Some(file) = codeowners::read(repo, &base_ref, forge, None)? else {
        return Ok(Vec::new());
    };
    let range = format!("{base_ref}...refs/heads/{branch}");
    let out = crate::git::run(
        Some(repo),
        &["diff", "--name-only", "-z", "--no-renames", &range, "--"],
    )?;
    let files = out.split('\0').filter(|p| !p.is_empty());
    Ok(codeowners::reviewers_for(&file, files)
        .into_iter()
        // glab asks accounts only.
        .filter(|r| forge != Some(Forge::GitLab) || !r.team)
        .collect())
}

/// The reviewers part of the _Create pull request_ dialog: the owners of what it changes, ticked
/// to be asked. `draft`: nobody is asked.
pub(crate) fn reviewers_ui(ui: &mut Ui, suggested: &mut [Suggested], draft: bool) {
    if suggested.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    let note = if draft {
        crate::i18n::text("repo-workflows-owners-reviewers-ui-796e0df8")
    } else {
        crate::i18n::text("repo-workflows-owners-reviewers-ui-da35c68e")
    };
    ui.label(RichText::new(note).color(t.text_dim).small());
    ui.add_enabled_ui(!draft, |ui| {
        ui.horizontal_wrapped(|ui| {
            for s in suggested.iter_mut() {
                let label = s.reviewer.label();
                let tip = if s.reviewer.team {
                    crate::i18n::text("repo-workflows-owners-reviewers-ui-7a8a3ca3")
                } else {
                    crate::i18n::text("repo-workflows-owners-reviewers-ui-d8e1731e")
                };
                ui.checkbox(&mut s.ask, label).on_hover_text(tip);
            }
        });
    });
}
