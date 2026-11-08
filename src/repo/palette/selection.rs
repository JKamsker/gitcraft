//! What the palette offers for what is selected: the commit picked in the graph, and the file
//! whose diff is open.

use std::path::PathBuf;

use super::{PaletteItem, PaletteRun, Pick};
use crate::git::{Area, StatusFile};
use crate::repo::commit_actions::{cherry_pick_action, revert_action};
use crate::repo::detail::{discard_dialog, stage_action};
use crate::repo::diff::unstage_args;
use crate::repo::file_view::FileTab;
use crate::repo::{Action, Dialog, DiffKey, Refresh, RepoTab, WIP_HASH, short};

impl RepoTab {
    /// The selected commit's entries, then the open file's.
    pub(super) fn selection_items(&self) -> Vec<PaletteItem> {
        let mut items = self.commit_items();
        items.extend(self.file_items());
        items.into_iter().map(PaletteItem::selection).collect()
    }

    /// Cherry-pick, revert, reset to, rebase onto, branch, tag or patch from the selected
    /// commit. Those that rewrite the checked-out branch wait for the running operation.
    fn commit_items(&self) -> Vec<PaletteItem> {
        let Some(commit) = self.selected.and_then(|i| self.commits.get(i)) else {
            return Vec::new();
        };
        if commit.hash == WIP_HASH {
            return Vec::new();
        }
        let hash = commit.hash.as_str();
        let at = short(hash);
        let is_head = hash == self.refs.head;
        let idle = self.busy_op.is_none();
        let target = self.refs.current.as_deref().unwrap_or("HEAD");
        let mut items = Vec::new();
        if idle && !is_head {
            items.push(
                PaletteItem::action(
                    format!("Cherry-pick {at}"),
                    "commit",
                    cherry_pick_action(hash, at),
                )
                .key("commit:cherry-pick"),
            );
        }
        if idle {
            items.push(
                PaletteItem::action(format!("Revert {at}"), "commit", revert_action(hash, at))
                    .key("commit:revert"),
            );
        }
        if idle && !is_head {
            items.push(
                PaletteItem::new(
                    format!("Reset {target} to here…"),
                    "commit",
                    PaletteRun::Pick(Pick::Reset(hash.to_owned())),
                )
                .key("commit:reset"),
            );
            items.push(
                PaletteItem::action(
                    format!("Interactive rebase {target} onto {at}…"),
                    "commit",
                    Action::InteractiveRebase {
                        onto: hash.to_owned(),
                        label: at.to_owned(),
                    },
                )
                .key("commit:interactive-rebase"),
            );
        }
        items.push(
            PaletteItem::action(
                format!("Create branch at {at}…"),
                "commit",
                Action::Dialog(Dialog::create_branch(hash, at)),
            )
            .key("commit:branch"),
        );
        items.push(
            PaletteItem::action(
                format!("Create tag at {at}…"),
                "commit",
                Action::Dialog(Dialog::tag(hash, at, self.default_remote())),
            )
            .key("commit:tag"),
        );
        if commit.parents.len() <= 1 {
            items.push(
                PaletteItem::action(
                    format!("Create patch from {at}…"),
                    "commit",
                    Action::SavePatch {
                        hash: hash.to_owned(),
                        summary: commit.summary.clone(),
                    },
                )
                .key("commit:patch"),
            );
        }
        // Menu visibility is presentation only; preserve every optional tool in the palette,
        // with the same selected-commit scope and eligibility as its menu counterpart.
        if idle {
            items.push(
                PaletteItem::action(
                    format!("Cut release at {at}…"),
                    "release",
                    Action::Release(crate::repo::release::ReleaseAct::Open(hash.to_owned())),
                )
                .key("commit:release"),
            );
        }
        if idle && self.surgery_refusal(hash).is_none() {
            items.push(
                PaletteItem::action(
                    format!("Split commit {at}…"),
                    "rewrite",
                    Action::OpenSplit(hash.to_owned()),
                )
                .key("commit:split"),
            );
            items.push(
                PaletteItem::action(
                    format!("Move changes from {at}…"),
                    "rewrite",
                    Action::OpenMove(hash.to_owned()),
                )
                .key("commit:move"),
            );
        }
        if self.bisect_blocker().is_none() {
            items.push(
                PaletteItem::action(
                    format!("Start bisect at {at}…"),
                    "bisect",
                    Action::Dialog(self.start_bisect_dialog(hash)),
                )
                .key("commit:bisect"),
            );
        }
        if !self.parallel.running() {
            items.push(
                PaletteItem::action(
                    format!("Bisect in parallel from {at}…"),
                    "bisect",
                    self.parallel_dialog_for(hash),
                )
                .key("commit:parallel-bisect"),
            );
        }
        if !self.test_lane.running() {
            items.push(
                PaletteItem::action(
                    format!("Test commits up to {at}…"),
                    "test",
                    self.test_dialog(Some(hash)),
                )
                .key("commit:test"),
            );
        }
        // Selection keys are stable identities. Project only the new visible captions,
        // using the already-selected immutable commit rather than parsing their text.
        for item in &mut items {
            let id = match item.key.as_str() {
                "commit:release" => "dev-baseline-selection-release",
                "commit:split" => "dev-baseline-selection-split",
                "commit:move" => "dev-baseline-selection-move",
                "commit:bisect" => "dev-baseline-selection-bisect",
                "commit:parallel-bisect" => "dev-baseline-selection-parallel-bisect",
                "commit:test" => "dev-baseline-selection-test",
                _ => continue,
            };
            item.label = crate::i18n::message(id, &[("commit", at.into())]);
        }
        items
    }

    /// Stage, unstage or discard the open working-tree file; view it, its history or its
    /// blame; open it.
    fn file_items(&self) -> Vec<PaletteItem> {
        let Some(key) = self.diff.as_ref().map(|d| &d.key) else {
            return Vec::new();
        };
        let path = key.path();
        let mut items = match key {
            DiffKey::Work(file) if self.busy_op.is_none() => self.work_file_items(file),
            _ => Vec::new(),
        };
        let rev = match key {
            DiffKey::Commit { hash, .. } | DiffKey::Remerge { hash, .. } => Some(hash.clone()),
            DiffKey::Range { to, .. } => to.clone(),
            DiffKey::Work(_) => None,
        };
        let deleted = matches!(key, DiffKey::Work(f) if f.code == 'D');
        if !deleted {
            for (label, tab, kind) in [
                (crate::i18n::text("repo-core-2996"), FileTab::File, "view"),
                (
                    crate::i18n::text("repo-core-2998"),
                    FileTab::History,
                    "history",
                ),
                (crate::i18n::text("repo-core-2498"), FileTab::Blame, "blame"),
            ] {
                items.push(
                    PaletteItem::action(
                        format!("{label} {path}"),
                        "file",
                        Action::FileView {
                            path: path.to_owned(),
                            rev: rev.clone(),
                            tab,
                        },
                    )
                    .key(format!("file:{kind}")),
                );
            }
        }
        if !deleted && rev.is_none() {
            let rel = PathBuf::from(path);
            items.push(
                PaletteItem::action(
                    format!("Open {path} in editor"),
                    "file",
                    Action::OpenInEditor(rel.clone()),
                )
                .key("file:editor"),
            );
            items.push(
                PaletteItem::action(format!("Open {path}"), "file", Action::OpenPath(rel))
                    .key("file:open"),
            );
        }
        items
    }

    /// Stage or discard a changed file, unstage a staged one.
    fn work_file_items(&self, file: &StatusFile) -> Vec<PaletteItem> {
        let path = &file.path;
        match file.area {
            Area::Unstaged | Area::Untracked => {
                let discard = if file.area == Area::Untracked {
                    crate::i18n::message("repo-core-3005", &[("path", path.to_string().into())])
                } else {
                    crate::i18n::message("repo-core-3006", &[("path", path.to_string().into())])
                };
                vec![
                    PaletteItem::action(
                        format!("Stage {path}"),
                        "file",
                        stage_action(
                            crate::i18n::text("repo-core-0836"),
                            vec![vec!["--literal-pathspecs", "add", "--", path]],
                        ),
                    )
                    .key("file:stage"),
                    PaletteItem::action(discard, "file", Action::Dialog(discard_dialog(file)))
                        .key("file:discard"),
                ]
            }
            Area::Staged => vec![
                PaletteItem::action(
                    format!("Unstage {path}"),
                    "file",
                    Action::Ops {
                        label: "Unstage".to_owned(),
                        ops: vec![unstage_args(&self.refs.head, path)],
                        refresh: Refresh::Status,
                    },
                )
                .key("file:unstage"),
            ],
            Area::Conflicted => Vec::new(),
        }
    }
}
