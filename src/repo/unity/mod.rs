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

use super::{Action, Refresh, RepoTab};
use crate::git::{self, MetaProblem, UnityProject, UnitySetup};

/// What the tab knows of its Unity project.
#[derive(Default)]
pub(in crate::repo) struct UnityState {
    project: Option<UnityProject>,
    /// The checkout it was looked for at (HEAD), and whether a look is running.
    detected_for: Option<String>,
    detecting: bool,
    setup: Option<Result<UnitySetup, String>>,
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
    busy: Option<String>,
    /// The hint that the set-up needs fixing was dismissed, for this tab.
    hint_dismissed: bool,
}

/// What the views ask of the Unity support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnityAct {
    OpenPanel,
    ClosePanel,
    /// Check the set-up again.
    Check,
    AddAttributes,
    AddIgnores,
    /// Pick UnityYAMLMerge by hand.
    ChooseTool,
    RegisterDriver,
    /// Let UnityYAMLMerge solve the open conflict.
    SolveConflict,
    DismissHint,
}

impl UnityState {
    pub(in crate::repo) fn project(&self) -> Option<&UnityProject> {
        self.project.as_ref()
    }

    /// Whether the conflict editor offers Smart Merge for `path`.
    pub(in crate::repo) fn offers_smart_merge(&self, path: &str) -> bool {
        self.project.is_some() && self.tool.is_some() && smart_merge::is_unity_yaml(path)
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
        Err(format!("Internal error: {said}"))
    })
}

/// What was found when looking for the project: it, and UnityYAMLMerge and the driver.
type Found = (Option<UnityProject>, Option<PathBuf>, Option<String>);

impl RepoTab {
    /// Every frame: the reads that are due, then the panel, if open.
    pub(in crate::repo) fn unity_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.unity_tick(ctx);
        self.unity_panel(ctx, acts);
    }

    /// Looks for the project when another commit is checked out; checks its set-up once found;
    /// checks the `.meta` pairs when the working tree was read again.
    pub(in crate::repo) fn unity_tick(&mut self, ctx: &egui::Context) {
        if self.loading || self.refs.head.is_empty() {
            return;
        }
        let state = &self.unity;
        if state.detected_for.as_deref() != Some(self.refs.head.as_str()) && !state.detecting {
            self.detect_unity(ctx);
        }
        if self.unity.project.is_none() {
            return;
        }
        if self.unity.setup.is_none() && !self.unity.checking {
            self.check_unity(ctx);
        }
        if self.unity.meta_for != Some(self.work_read) {
            self.check_metas(ctx);
        }
    }

    fn detect_unity(&mut self, ctx: &egui::Context) {
        self.unity.detecting = true;
        let head = self.refs.head.clone();
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    let project = git::unity_project(&path)?;
                    let (tool, driver) = match &project {
                        Some(p) => {
                            let configured =
                                git::run(Some(&path), &["config", "--get", smart_merge::TOOL_KEY])
                                    .ok()
                                    .map(|v| v.trim().to_owned());
                            let tool =
                                smart_merge::find_tool(p.version.as_deref(), configured.as_deref());
                            (tool, smart_merge::registered(&path))
                        }
                        None => (None, None),
                    };
                    Ok::<Found, String>((project, tool, driver))
                })
            },
            move |tab, found, _| {
                let state = &mut tab.unity;
                state.detecting = false;
                state.detected_for = Some(head);
                let (project, tool, driver) = found.unwrap_or_default();
                if project != state.project {
                    state.setup = None;
                    state.meta.clear();
                    state.meta_for = None;
                }
                state.project = project;
                state.tool = tool;
                state.driver = driver;
            },
        );
    }

    fn check_unity(&mut self, ctx: &egui::Context) {
        let Some(project) = self.unity.project.clone() else {
            return;
        };
        self.unity.checking = true;
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

    /// Checks the `.meta` pairs of the staged changes; one check at a time.
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
        let staged = self.work.staged.clone();
        let candidates = git::meta_candidates(&project, &staged);
        if candidates.is_empty() {
            self.unity.meta.clear();
            return;
        }
        self.unity.meta_running = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    let tracked = git::in_index(&path, &candidates)?;
                    Ok(git::meta_problems(&project, &staged, &|p| {
                        tracked.contains(p)
                    }))
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
                self.unity.setup = None;
            }
            UnityAct::ClosePanel => self.unity.panel_open = false,
            UnityAct::Check => {
                self.unity.setup = None;
                self.unity.detected_for = None;
            }
            UnityAct::DismissHint => self.unity.hint_dismissed = true,
            UnityAct::AddAttributes => self.add_attributes(ctx),
            UnityAct::AddIgnores => self.add_ignores(ctx),
            UnityAct::ChooseTool => self.choose_tool(ctx),
            UnityAct::RegisterDriver => self.register_driver(ctx),
            UnityAct::SolveConflict => self.smart_merge_conflict(ctx),
        }
    }

    /// Runs `work` in the background, says how it went, then checks the set-up again.
    fn unity_job(
        &mut self,
        label: String,
        ctx: &egui::Context,
        work: impl FnOnce(&std::path::Path) -> Result<(), String> + Send + 'static,
    ) {
        if self.unity.busy.is_some() {
            return;
        }
        self.unity.busy = Some(label.clone());
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| work(&path)),
            move |tab, result, ctx| {
                tab.unity.busy = None;
                tab.status = Some(match result {
                    Ok(()) => (format!("{label}: done"), false),
                    Err(e) => (format!("{label} failed: {e}"), true),
                });
                tab.unity.setup = None;
                tab.unity.detected_for = None;
                tab.reload_status(ctx);
            },
        );
    }

    fn add_attributes(&mut self, ctx: &egui::Context) {
        let Some(Ok(setup)) = &self.unity.setup else {
            return;
        };
        let block = git::attributes_block(&setup.missing_attributes);
        self.unity_job("Add Unity's .gitattributes".to_owned(), ctx, move |repo| {
            git::append_block(repo, ".gitattributes", &block)
        });
    }

    fn add_ignores(&mut self, ctx: &egui::Context) {
        let (Some(Ok(setup)), Some(project)) = (&self.unity.setup, &self.unity.project) else {
            return;
        };
        let block = git::ignore_block(project, &setup.unignored);
        self.unity_job(
            "Ignore Unity's generated folders".to_owned(),
            ctx,
            move |repo| git::append_block(repo, ".gitignore", &block),
        );
    }

    /// Asks for UnityYAMLMerge with a file dialog (as the app's other dialogs are asked), and
    /// keeps the choice in the repository's config.
    fn choose_tool(&mut self, ctx: &egui::Context) {
        let Some(file) = rfd::FileDialog::new()
            .set_title("UnityYAMLMerge (in the editor's Tools folder)")
            .pick_file()
        else {
            return;
        };
        let name = file.file_name().map(|n| n.to_string_lossy().to_lowercase());
        if !name.is_some_and(|n| n.starts_with("unityyamlmerge")) {
            self.status = Some((format!("{} isn't UnityYAMLMerge", file.display()), true));
            return;
        }
        let value = file.to_string_lossy().into_owned();
        self.unity_job("Choose UnityYAMLMerge".to_owned(), ctx, move |repo| {
            git::run(
                Some(repo),
                &["config", "--local", smart_merge::TOOL_KEY, &value],
            )
            .map(|_| ())
        });
    }

    fn register_driver(&mut self, ctx: &egui::Context) {
        let Some(tool) = self.unity.tool.clone() else {
            self.status = Some((
                "UnityYAMLMerge wasn't found: choose it first".to_owned(),
                true,
            ));
            return;
        };
        self.unity_job("Register Unity Smart Merge".to_owned(), ctx, move |repo| {
            smart_merge::register(repo, &tool)
        });
    }

    /// Lets UnityYAMLMerge solve the open conflict; its result goes into the conflict editor.
    fn smart_merge_conflict(&mut self, ctx: &egui::Context) {
        let Some(tool) = self.unity.tool.clone() else {
            self.status = Some(("UnityYAMLMerge wasn't found".to_owned(), true));
            return;
        };
        let Some(ed) = self.conflict.as_mut().filter(|ed| !ed.solving) else {
            return;
        };
        ed.solving = true;
        let (repo, file) = (self.path.clone(), ed.path.clone());
        self.then(
            ctx,
            move || caught(|| smart_merge::solve(&repo, &file, &tool)).map(|t| (file, t)),
            |tab, result, _| tab.smart_merge_done(result),
        );
    }

    fn smart_merge_done(&mut self, result: Result<(String, String), String>) {
        let Some(ed) = self.conflict.as_mut() else {
            return;
        };
        let (path, text) = match result {
            Ok(done) => done,
            Err(e) => {
                ed.solving = false;
                self.status = Some((e, true));
                return;
            }
        };
        if ed.path != path {
            return;
        }
        ed.solving = false;
        let before = ed.on_disk;
        let left = ed.take_solved(text);
        let solved = before.saturating_sub(left);
        self.status = Some(match (solved, left) {
            (_, 0) => (
                format!("UnityYAMLMerge merged {path}: check the result, then save"),
                false,
            ),
            (0, _) => (
                format!("UnityYAMLMerge could not solve the conflicts in {path}"),
                true,
            ),
            _ => (
                format!(
                    "UnityYAMLMerge solved {solved} of {before} conflicts in {path}; {left} left to choose"
                ),
                false,
            ),
        });
    }

    /// Stages the `.meta` files (or assets) the staged changes are missing, those that have
    /// changes to stage.
    pub(in crate::repo) fn stage_partners_action(&self) -> Option<Action> {
        let unstaged: HashSet<&str> = self.work.unstaged.iter().map(|f| f.path.as_str()).collect();
        let partners: Vec<&str> = self
            .unity
            .meta
            .iter()
            .map(|p| p.partner.as_str())
            .filter(|p| unstaged.contains(p))
            .collect();
        if partners.is_empty() {
            return None;
        }
        let mut add = vec!["--literal-pathspecs", "add", "--"];
        add.extend(partners);
        Some(Action::Ops {
            label: "Stage the .meta files".to_owned(),
            ops: super::ops(&[&add]),
            refresh: Refresh::Status,
        })
    }
}
