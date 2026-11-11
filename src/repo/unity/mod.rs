//! Unity projects in the tab: found once the repository is read, their set-up for git checked
//! (Force Text, visible `.meta` files, `.gitattributes`, `.gitignore`) with fixes for what gitcraft
//! can fix, assets staged without their `.meta` (or the other way round) flagged above the
//! changes, and Unity's Smart Merge registered and offered in the conflict editor.

mod banner;
mod panel;
pub(super) mod smart_merge;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::PathBuf;

use super::{Action, Refresh, RepoTab, ops};
use crate::git::{self, MetaKind, MetaProblem, UnityProject, UnitySetup};

/// The repository's own config key that turns the set-up hint off.
const HINT_KEY: &str = "gitcraft.unityHint";

/// What the tab knows of its Unity project.
#[derive(Default)]
pub(in crate::repo) struct UnityState {
    project: Option<UnityProject>,
    /// The checkout it was looked for at (HEAD, or "unborn" before the first commit), and
    /// whether a look is running.
    detected_for: Option<String>,
    detecting: bool,
    /// The set-up as last checked; `stale`: it is to be checked again (it stays shown
    /// meanwhile).
    setup: Option<Result<UnitySetup, String>>,
    stale: bool,
    checking: bool,
    /// UnityYAMLMerge, here; and the merge driver the repository registers, if any.
    tool: Option<PathBuf>,
    driver: Option<String>,
    /// Assets staged without their `.meta` (or the other way round), for the working-tree
    /// read numbered so.
    meta: Vec<MetaProblem>,
    meta_for: Option<u64>,
    meta_running: bool,
    meta_again: bool,
    panel_open: bool,
    /// What is being done, as the panel says it ("Adding lines to .gitattributes").
    busy: Option<String>,
    /// The set-up hint was turned off for this repository (`gitcraft.unityHint false`).
    hint_off: bool,
}

/// What the views ask of the Unity support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnityAct {
    OpenPanel,
    ClosePanel,
    /// Check the set-up again.
    Check,
    /// Add the Smart Merge lines (`lfs: false`) or the LFS lines to `.gitattributes`.
    AddAttributes {
        lfs: bool,
    },
    AddIgnores,
    /// Take the generated folders committed by mistake out of the index.
    UntrackGenerated,
    /// Pick UnityYAMLMerge by hand.
    ChooseTool,
    RegisterDriver,
    /// Let UnityYAMLMerge solve the open conflict.
    SolveConflict,
    /// Turn the set-up hint off for this repository.
    DismissHint,
}

impl UnityState {
    pub(in crate::repo) fn project(&self) -> Option<&UnityProject> {
        self.project.as_ref()
    }

    /// For a Unity YAML file of the project: whether the conflict editor can solve it with
    /// Smart Merge (UnityYAMLMerge was found); `None` for any other file.
    pub(in crate::repo) fn smart_merge_for(&self, path: &str) -> Option<bool> {
        (self.project.is_some() && smart_merge::is_unity_yaml(path)).then_some(self.tool.is_some())
    }

    /// What the set-up hint names, when it shows.
    fn hint(&self) -> Option<String> {
        match &self.setup {
            Some(Ok(s)) if !self.hint_off => s.needs_fixing(),
            _ => None,
        }
    }
}

/// `work`'s answer, or what it said if it panicked: the flag waiting for the answer is let go
/// of all the same.
fn caught<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|p| {
        let said = p
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(crate::i18n::message(
            "repo-workflows-unity-caught-ff2cd16f",
            &[("said", (said).clone().into())],
        ))
    })
}

/// What was found when looking for the project.
#[derive(Default)]
struct Found {
    project: Option<UnityProject>,
    tool: Option<PathBuf>,
    driver: Option<String>,
    hint_off: bool,
}

impl RepoTab {
    /// Every frame: the reads that are due, then the panel, if open.
    pub(in crate::repo) fn unity_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.unity_tick(ctx);
        self.unity_panel(ctx, acts);
    }

    /// The checkout the project is looked for at: HEAD, or "unborn" before the first commit.
    fn checkout_key(&self) -> String {
        if self.refs.head.is_empty() {
            "unborn".to_owned()
        } else {
            self.refs.head.clone()
        }
    }

    /// Looks for the project, and checks its set-up again, whenever another commit is checked
    /// out; checks the `.meta` pairs when the working tree was read again.
    pub(in crate::repo) fn unity_tick(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        let key = self.checkout_key();
        if self.unity.detected_for.as_deref() != Some(key.as_str()) && !self.unity.detecting {
            self.detect_unity(key, ctx);
        }
        if self.unity.project.is_none() {
            return;
        }
        if (self.unity.setup.is_none() || self.unity.stale) && !self.unity.checking {
            self.check_unity(ctx);
        }
        if self.unity.meta_for != Some(self.work_read) {
            self.check_metas(ctx);
        }
    }

    fn detect_unity(&mut self, key: String, ctx: &egui::Context) {
        self.unity.detecting = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| find(&path)),
            move |tab, found, _| {
                let state = &mut tab.unity;
                state.detecting = false;
                state.detected_for = Some(key);
                let found = found.unwrap_or_default();
                if found.project != state.project {
                    state.setup = None;
                    state.meta.clear();
                    state.meta_for = None;
                }
                // Another checkout may have other settings and attributes.
                state.stale = true;
                state.project = found.project;
                state.tool = found.tool;
                state.driver = found.driver;
                state.hint_off = found.hint_off;
            },
        );
    }

    fn check_unity(&mut self, ctx: &egui::Context) {
        let Some(project) = self.unity.project.clone() else {
            return;
        };
        self.unity.checking = true;
        self.unity.stale = false;
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| git::unity_setup(&path, &project)),
            |tab, setup, _| {
                tab.unity.checking = false;
                tab.unity.setup = Some(setup);
            },
        );
    }

    /// Checks the `.meta` pairs of the staged changes under `Assets/`, against one listing of
    /// the index there; one check at a time.
    fn check_metas(&mut self, ctx: &egui::Context) {
        if self.unity.meta_running {
            self.unity.meta_again = true;
            return;
        }
        let read = self.work_read;
        self.unity.meta_for = Some(read);
        let Some(project) = self.unity.project.clone() else {
            return;
        };
        let assets = project.path("Assets/");
        let staged: Vec<git::StatusFile> = self
            .work
            .staged
            .iter()
            .filter(|f| {
                matches!(f.code, 'A' | 'R' | 'C' | 'D')
                    && (f.path.starts_with(&assets)
                        || f.old_path
                            .as_deref()
                            .is_some_and(|p| p.starts_with(&assets)))
            })
            .cloned()
            .collect();
        if staged.is_empty() {
            self.unity.meta.clear();
            return;
        }
        self.unity.meta_running = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    let index = git::index_under(&path, &assets)?;
                    Ok(git::meta_problems(&project, &staged, &|p| index.has(p)))
                })
            },
            move |tab, problems, ctx| {
                let state = &mut tab.unity;
                state.meta_running = false;
                if state.meta_for == Some(read) {
                    // A failure leaves no warning rather than a wrong one.
                    state.meta = problems.unwrap_or_default();
                }
                if std::mem::take(&mut tab.unity.meta_again) {
                    tab.unity.meta_for = None;
                    tab.check_metas(ctx);
                }
            },
        );
    }

    pub(in crate::repo) fn apply_unity_act(&mut self, act: UnityAct, ctx: &egui::Context) {
        match act {
            UnityAct::OpenPanel => {
                self.unity.panel_open = true;
                self.unity.stale = true;
            }
            UnityAct::ClosePanel => self.unity.panel_open = false,
            UnityAct::Check => self.unity.detected_for = None,
            UnityAct::DismissHint => {
                self.unity.hint_off = true;
                let path = self.path.clone();
                self.then(
                    ctx,
                    move || git::run(Some(&path), &["config", "--local", HINT_KEY, "false"]),
                    |_, _, _| {},
                );
            }
            UnityAct::AddAttributes { lfs } => self.add_attributes(lfs, ctx),
            UnityAct::AddIgnores => self.add_ignores(ctx),
            UnityAct::UntrackGenerated => self.untrack_generated(ctx),
            UnityAct::ChooseTool => self.choose_tool(ctx),
            UnityAct::RegisterDriver => self.register_driver(ctx),
            UnityAct::SolveConflict => self.smart_merge_conflict(ctx),
        }
    }

    /// The fixes for the `.meta` problems, each with its button's label: stage the missing
    /// halves that are there to stage, delete the `.meta` of a deleted asset too, keep the
    /// `.meta` of a kept asset.
    pub(in crate::repo) fn meta_fixes(&self) -> Vec<(&'static str, Action)> {
        let changed: HashSet<&str> = self.work.unstaged.iter().map(|f| f.path.as_str()).collect();
        let of = |kinds: &[MetaKind], partner: bool| -> Vec<&str> {
            self.unity
                .meta
                .iter()
                .filter(|p| kinds.contains(&p.kind))
                .map(|p| {
                    if partner {
                        p.partner.as_str()
                    } else {
                        p.path.as_str()
                    }
                })
                .collect()
        };
        let mut fixes = Vec::new();
        let stage: Vec<&str> = of(
            &[MetaKind::AddedWithoutMeta, MetaKind::MetaWithoutAsset],
            true,
        )
        .into_iter()
        .filter(|p| changed.contains(p))
        .collect();
        if !stage.is_empty() {
            let mut add = vec!["--literal-pathspecs", "add", "--"];
            add.extend(stage);
            fixes.push((
                crate::i18n::text("repo-workflows-unity-meta-fixes-3bb415b3"),
                op(
                    crate::i18n::text("repo-workflows-unity-meta-fixes-3bb415b3"),
                    &add,
                ),
            ));
        }
        let delete = of(&[MetaKind::DeletedWithoutMeta], true);
        if !delete.is_empty() {
            let mut rm = vec!["--literal-pathspecs", "rm", "-q", "--"];
            rm.extend(delete);
            fixes.push((
                crate::i18n::text("repo-workflows-unity-meta-fixes-a2c5406b"),
                op(
                    crate::i18n::text("repo-workflows-unity-meta-fixes-ff8580ac"),
                    &rm,
                ),
            ));
        }
        let restore = of(&[MetaKind::MetaDeletedAssetKept], false);
        if !restore.is_empty() {
            let mut back = vec![
                "--literal-pathspecs",
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
            ];
            back.extend(restore);
            fixes.push((
                crate::i18n::text("repo-workflows-unity-meta-fixes-8f9d320c"),
                op(
                    crate::i18n::text("repo-workflows-unity-meta-fixes-e0871b1a"),
                    &back,
                ),
            ));
        }
        fixes
    }
}

/// Looks for the project, UnityYAMLMerge, the registered driver and whether the hint is off.
fn find(path: &std::path::Path) -> Result<Found, String> {
    let Some(project) = git::unity_project(path)? else {
        return Ok(Found::default());
    };
    let config = |k: &str| {
        git::run(Some(path), &["config", "--get", k])
            .ok()
            .map(|v| v.trim().to_owned())
    };
    let tool = smart_merge::find_tool(
        project.version.as_deref(),
        config(smart_merge::TOOL_KEY).as_deref(),
    );
    Ok(Found {
        driver: smart_merge::registered(path),
        hint_off: config(HINT_KEY).as_deref() == Some("false"),
        project: Some(project),
        tool,
    })
}

fn op(label: &str, args: &[&str]) -> Action {
    Action::Ops {
        label: label.to_owned(),
        ops: ops(&[args]),
        refresh: Refresh::Status,
    }
}

mod merge;
mod setup;
