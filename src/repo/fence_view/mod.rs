//! The agent fence in the tab: its list read with the repository, every linked worktree
//! checked against it in the background (a worktree touching it turns red in the sidebar),
//! the window that edits it and writes it into the agents' configs, and the confirmation a
//! merge of a red worktree's branch needs.

mod editor;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;

use super::{Action, Dialog, RepoTab};
use crate::fence::{self, Violation};
use crate::git::Worktree;

pub(crate) use editor::FenceEditor;

/// Added to the label of a merge confirmed past the fence: it reads in the undo list as what it
/// was, and the gate lets it through.
const PAST: &str = " (past the agent fence)";

/// What the tab knows of the fence.
#[derive(Default)]
pub(crate) struct FenceState {
    /// The list, as read last.
    pub(crate) list: Vec<String>,
    /// Which configurations besides Claude Code's it is written into.
    pub(crate) targets: fence::Targets,
    /// The list was read at least once.
    pub(crate) loaded: bool,
    /// Each linked worktree checked (by its folder): the fenced paths it touched, or why it
    /// couldn't be checked (which counts as touched: a guardrail errs on the safe side).
    pub(crate) found: HashMap<PathBuf, Result<Vec<Violation>, String>>,
    /// Writing the agents' configs into a worktree failed: where, and why.
    pub(crate) write_errors: Vec<String>,
    write_error_context: Vec<RootFailure>,
    checking: bool,
    /// Checked again once this check is in (the repository was read again meanwhile).
    again: bool,
    /// Steps a background check of their branch found clean: let through once.
    cleared: Option<Vec<Vec<String>>>,
    pub(crate) editor: Option<FenceEditor>,
}

impl FenceState {
    /// The fenced paths the worktree at `path` touched (none when not checked or clean).
    pub(crate) fn violations(&self, path: &std::path::Path) -> &[Violation] {
        match self.found.get(path) {
            Some(Ok(v)) => v,
            _ => &[],
        }
    }

    /// Why the worktree at `path` couldn't be checked, if it couldn't.
    pub(crate) fn error(&self, path: &std::path::Path) -> Option<&str> {
        match self.found.get(path) {
            Some(Err(e)) => Some(e),
            _ => None,
        }
    }

    /// Whether the worktree at `path` is red: it touched the fence, or couldn't be checked.
    pub(crate) fn red(&self, path: &std::path::Path) -> bool {
        !self.violations(path).is_empty() || self.error(path).is_some()
    }
}

/// The files gitcraft writes into a worktree itself, left out of its check.
fn own_files(targets: fence::Targets) -> Vec<&'static str> {
    let mut own = vec![fence::CLAUDE];
    if targets.cursor {
        own.push(fence::CURSOR);
    }
    if targets.agents_md {
        own.push(fence::AGENTS);
    }
    own
}

/// The worktrees checked: the linked ones there are, not the main one.
fn checked(worktrees: &[Worktree]) -> Vec<Worktree> {
    worktrees
        .iter()
        .filter(|w| !w.main && !w.bare && w.prunable.is_none() && !w.head.is_empty())
        .cloned()
        .collect()
}

/// What a check of the worktrees found.
struct Checked {
    list: Vec<String>,
    targets: fence::Targets,
    found: HashMap<PathBuf, Result<Vec<Violation>, String>>,
    write_errors: Vec<String>,
    write_error_context: Vec<RootFailure>,
}

#[derive(Clone)]
struct RootFailure {
    root: String,
    detail: String,
}
impl RootFailure {
    fn new(root: &std::path::Path, detail: String) -> Self {
        Self {
            root: crate::remote::display(root),
            detail,
        }
    }
    fn raw(&self) -> String {
        format!("{}: {}", self.root, self.detail)
    }
    fn display(&self) -> String {
        format!(
            "{}: {}",
            self.root,
            fence::display::diagnostic(&self.detail)
        )
    }
}
fn display_write_errors(raw: &[String], context: &[RootFailure]) -> String {
    raw.iter()
        .enumerate()
        .map(|(index, error)| {
            context
                .get(index)
                .filter(|source| source.raw() == *error)
                .map_or_else(|| error.clone(), RootFailure::display)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl RepoTab {
    /// Reads the list, writes it into the agents' configs of worktrees that lack it (one
    /// added since it was saved), and checks the linked worktrees, in the background.
    pub(crate) fn check_fence(&mut self, ctx: &egui::Context) {
        if self.fence.checking {
            self.fence.again = true;
            return;
        }
        let worktrees = checked(&self.worktrees);
        let roots: Vec<PathBuf> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .map(|w| w.path.clone())
            .collect();
        let base = self
            .worktrees
            .iter()
            .find(|w| w.main)
            .map(|w| w.head.clone())
            .unwrap_or_default();
        self.fence.checking = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let list = fence::read(&repo)?;
                let targets = fence::read_targets(&repo);
                let patterns = fence::parse_list(&list);
                let mut write_errors = Vec::new();
                let mut write_error_context = Vec::new();
                if !patterns.is_empty() {
                    for root in &roots {
                        if let Err(e) = fence::write_all(root, &patterns, targets) {
                            write_errors.push(format!("{}: {e}", crate::remote::display(root)));
                            write_error_context.push(RootFailure::new(root, e));
                        }
                    }
                }
                let own = own_files(targets);
                let found = worktrees
                    .iter()
                    .map(|w| {
                        (
                            w.path.clone(),
                            fence::check(&w.path, &base, &patterns, &own),
                        )
                    })
                    .collect();
                Ok::<_, String>(Checked {
                    list,
                    targets,
                    found,
                    write_errors,
                    write_error_context,
                })
            },
            |tab, result, ctx| {
                tab.fence.checking = false;
                match result {
                    Ok(c) => {
                        tab.fence.list = c.list;
                        tab.fence.targets = c.targets;
                        tab.fence.found = c.found;
                        tab.fence.write_errors = c.write_errors;
                        tab.fence.write_error_context = c.write_error_context;
                        tab.fence.loaded = true;
                    }
                    Err(e) => tab.status = Some((e, true)),
                }
                if std::mem::take(&mut tab.fence.again) {
                    tab.check_fence(ctx);
                }
            },
        );
    }

    /// `action`, or nothing yet: a merge, fast-forward or rebase onto a branch is first
    /// checked against the fence in the background (what its commits bring in, and what its
    /// worktree, if it has one, touched); it then runs, or asks to be confirmed.
    pub(crate) fn fence_gate(&mut self, action: Action, ctx: &egui::Context) -> Option<Action> {
        let Action::UndoableOps {
            label,
            ops,
            refresh,
            undo,
        } = action
        else {
            return Some(action);
        };
        let pass = Action::UndoableOps {
            label: label.clone(),
            ops: ops.clone(),
            refresh,
            undo: undo.clone(),
        };
        if label.ends_with(PAST) || self.fence.cleared.as_ref() == Some(&ops) {
            self.fence.cleared = None;
            return Some(pass);
        }
        let Some(tip) = brought_in(&ops).map(str::to_owned) else {
            return Some(pass);
        };
        if self.fence.loaded && fence::parse_list(&self.fence.list).is_empty() {
            return Some(pass);
        }
        // What its worktree touched, if one has the branch checked out.
        let short = tip
            .strip_prefix("refs/heads/")
            .or_else(|| {
                tip.strip_prefix("refs/remotes/")
                    .and_then(|r| r.split_once('/').map(|(_, b)| b))
            })
            .unwrap_or(&tip)
            .to_owned();
        let worktree = self
            .worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some(short.as_str()))
            .map(|w| {
                let found = match self.fence.found.get(&w.path) {
                    Some(Ok(v)) => Ok(v.clone()),
                    Some(Err(e)) => Err(e.clone()),
                    None => Ok(Vec::new()),
                };
                (w.name(), found, crate::remote::display(&w.path))
            });
        if self.busy_op.is_some() {
            return Some(pass);
        }
        self.busy_op = Some("Checking against the agent fence".to_owned());
        self.status = Some(("Checking against the agent fence…".to_owned(), false));
        let repo = self.path.clone();
        let display_tip = tip.clone();
        self.then(
            ctx,
            move || {
                let patterns = fence::parse_list(&fence::read(&repo)?);
                if patterns.is_empty() {
                    return Ok(Vec::new());
                }
                fence::check_ref(&repo, &tip, "HEAD", &patterns)
            },
            move |tab, commits, ctx| {
                tab.busy_op = None;
                tab.status = None;
                let mut found: Vec<Violation> = Vec::new();
                let mut errors: Vec<String> = Vec::new();
                match commits {
                    Ok(v) => found.extend(v),
                    Err(e) => errors.push(fence::display::check_diagnostic(&e, &display_tip)),
                }
                if let Some((_, wt, target)) = &worktree {
                    match wt {
                        Ok(v) => found.extend(
                            v.iter()
                                .filter(|x| !found.contains(x))
                                .cloned()
                                .collect::<Vec<_>>(),
                        ),
                        Err(e) => errors.push(fence::display::check_diagnostic(e, target)),
                    }
                }
                if found.is_empty() && errors.is_empty() {
                    tab.fence.cleared = Some(ops.clone());
                    tab.apply(
                        Action::UndoableOps {
                            label,
                            ops,
                            refresh,
                            undo,
                        },
                        ctx,
                    );
                    return;
                }
                let body = confirm_body(
                    &short,
                    worktree.as_ref().map(|w| w.0.as_str()),
                    &found,
                    &errors,
                );
                tab.dialog = Some(
                    Dialog::confirm(
                        &format!("{label}{PAST}"),
                        body,
                        crate::i18n::text("repo-workflows-fence_view-fence-gate-214a3091"),
                        ops,
                        None,
                    )
                    .undo(undo),
                );
            },
        );
        None
    }
}

/// The ref a `merge` or `rebase` step brings in, if the first step is one.
fn brought_in(ops: &[Vec<String>]) -> Option<&str> {
    let first = ops.first()?;
    if !matches!(first.first().map(String::as_str), Some("merge" | "rebase")) {
        return None;
    }
    let last = first.last()?;
    last.starts_with("refs/").then_some(last.as_str())
}

/// Most fenced paths a hover or a confirmation lists.
const LISTED: usize = 8;

/// The fenced paths, one per line, `how` each was touched, cut to [`LISTED`].
pub(crate) fn list_violations(found: &[Violation]) -> String {
    let mut out: Vec<String> = found
        .iter()
        .take(LISTED)
        .map(|v| {
            crate::i18n::message(
                "repo-workflows-fence-violation-line",
                &[
                    ("path", (v.path.as_str()).into()),
                    (
                        "how",
                        (match v.how {
                            fence::How::Committed => "committed",
                            fence::How::Uncommitted => "uncommitted",
                            fence::How::Untracked => "untracked",
                            fence::How::Ignored => "ignored",
                        })
                        .into(),
                    ),
                    ("pattern", (v.pattern.as_str()).into()),
                ],
            )
        })
        .collect();
    if found.len() > LISTED {
        out.push(crate::i18n::message(
            "repo-workflows-fence_view-list-violations-a6d5a69b",
            &[("found-len-listed-1", (found.len() - LISTED).clone().into())],
        ));
    }
    out.join("\n")
}

fn confirm_body(
    branch: &str,
    worktree: Option<&str>,
    found: &[Violation],
    errors: &[String],
) -> String {
    crate::i18n::message(
        "repo-workflows-fence-confirm-body",
        &[
            ("branch", (branch).into()),
            (
                "worktree",
                (if worktree.is_some() { "yes" } else { "no" }).into(),
            ),
            ("folder", (worktree.unwrap_or_default()).into()),
            (
                "found",
                (if found.is_empty() { "no" } else { "yes" }).into(),
            ),
            ("paths", (list_violations(found)).into()),
            (
                "errors",
                (if errors.is_empty() { "no" } else { "yes" }).into(),
            ),
            (
                "details",
                (errors.join(
                    "
  ",
                ))
                .into(),
            ),
        ],
    )
}
#[cfg(test)]
mod fluent_confirmation_tests {
    use super::*;
    #[test]
    fn fluent_partition_fence_actual_confirmation_keeps_paths_and_checks_raw() {
        let violation = Violation {
            path: "secret/ß file".into(),
            pattern: "secret/**".into(),
            how: fence::How::Untracked,
        };
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            for has_found in [false, true] {
                for has_errors in [false, true] {
                    for worktree in [None, Some("/supplied/ß tree")] {
                        let found = if has_found {
                            vec![violation.clone()]
                        } else {
                            vec![]
                        };
                        let errors = if has_errors {
                            vec!["Opaque provider detail 日本語".into()]
                        } else {
                            vec![]
                        };
                        let body = confirm_body("feature/ß", worktree, &found, &errors);
                        assert!(body.starts_with("feature/ß"));
                        assert_eq!(body.contains("/supplied/ß tree"), worktree.is_some());
                        assert_eq!(body.contains("secret/ß file"), has_found);
                        assert_eq!(body.contains("secret/**"), has_found);
                        assert_eq!(body.contains("Opaque provider detail 日本語"), has_errors);
                        assert!(body.ends_with(if language == "de" {
                            "Trotzdem fortfahren?"
                        } else {
                            "Go ahead all the same?"
                        }));
                        if has_found {
                            assert!(body.contains(if language == "de" {
                                "Agenten nicht verändern dürfen"
                            } else {
                                "agents must stay out of"
                            }));
                            assert!(body.contains(if language == "de" {
                                "nicht versioniert; Grenze:"
                            } else {
                                "untracked; fence:"
                            }));
                        }
                        if has_errors {
                            assert!(body.contains(if language == "de" {
                                "konnte"
                            } else {
                                "could not"
                            }));
                        }
                        assert_eq!(violation.how.label(), "untracked");
                        assert_eq!(found.len(), usize::from(has_found));
                        assert_eq!(errors.len(), usize::from(has_errors));
                    }
                }
            }
        }
        crate::i18n::set_language("en-US");
    }
}

#[cfg(test)]
mod fluent_guard_target_tests {
    use super::*;
    #[test]
    fn fluent_partition_fence_supplied_target_preserves_punctuation_and_raw_gate_identity() {
        let target = "/supplied/x against the agent fence: y/ß";
        let raw = format!(
            "Could not check {target} against the agent fence: there is no main branch to compare with"
        );
        for lang in ["en-US", "de"] {
            crate::i18n::set_language(lang);
            let errors = vec![fence::display::check_diagnostic(&raw, target)];
            let body = confirm_body("refs/heads/ß", Some(target), &[], &errors);
            assert!(body.contains(target));
            assert_eq!(body.contains("keinen Hauptbranch"), lang == "de");
            assert_eq!(fence::display::diagnostic(&raw), raw);
            assert_eq!(fence::display::check_diagnostic(&raw, "different"), raw);
            assert!(format!("Merge refs/heads/ß{PAST}").ends_with(PAST));
            assert_eq!(PAST, " (past the agent fence)");
        }
        crate::i18n::set_language("en-US");
    }
}
