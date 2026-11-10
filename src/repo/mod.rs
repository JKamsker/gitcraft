//! One open repository: its state, background git work and the view
//! (branches on the left, commit graph or diff in the middle, commit detail / changes on the right).

mod notice;
pub(crate) use notice::Notice;
mod actions;
mod activity;
pub(crate) mod agent;
mod agents;
mod ai;
mod apply;
mod atlas;
mod authorship_view;
mod bakeoff;
mod bisect;
mod branch_bands;
mod branch_cleanup;
mod branch_health;
mod branch_order;
mod branch_tree;
mod branch_visibility;
mod bundles;
mod celebration;
mod ci_status;
mod ci_view;
mod cleanup;
mod cochanges;
mod collision_view;
pub(crate) mod columns;
mod commands;
mod commit_actions;
mod commit_changes;
pub(crate) mod conflict;
mod conflict_check;
mod content_recovery;
#[cfg(test)]
mod cpu_benchmarks;
pub(crate) mod custom_commands;
mod dco;
mod detached;
mod detail;
mod detail_cache;
mod devcontainer;
mod dialogs;
mod diff;
mod diff_split;
pub(crate) mod doctor;
mod drafts;
#[cfg(test)]
mod drafts_tests;
mod drag_drop;
mod drift_view;
mod email;
mod error_help;
mod external_tools;
pub(crate) mod fence_view;
pub(crate) mod file_comparison;
mod file_text;
pub(crate) mod file_view;
mod find;
mod fold;
mod folding;
mod forge_link;
mod forge_selection;
pub(crate) mod forks;
mod gerrit;
mod ghost;
mod graph_motion;
mod graph_order;
pub(crate) mod graph_view;
mod graph_walk;
mod growth;
mod guide;
pub(crate) mod highlight;
mod history_activity;
pub(crate) mod history_filter;
mod history_reader;
mod hooks;
mod hotfix;
mod https_recovery;
mod insights;
mod interaction;
pub(crate) mod issue_work;
mod keep_submodules;
pub(crate) mod keyboard;
mod keys;
mod layout;
mod lfs_locks;
mod lfs_storage;
mod lifecycle;
mod line_dossier;
#[cfg(test)]
mod line_history_tests;
mod load;
mod local_ci;
mod messages;
#[cfg(not(test))]
pub(crate) use messages::discover_fetch_key;
pub(crate) use messages::{fetch_key, mutation_key};
mod nav_history;
mod navigate;
mod navigation;
mod offline;
pub(crate) mod operations;
pub(crate) use operations::run_reviewed_push;
mod owners;
mod package;
mod palette;
mod parallel_bisect;
mod partial_stash;
mod patches;
mod pinned_sections;
pub(crate) mod popout;
mod postcards;
mod pr_merge;
mod pr_review;
mod pr_view;
mod practice;
#[cfg(test)]
mod process_benchmarks;
pub(crate) mod pull;
mod push;
mod push_preflight;
mod quick_edit;
mod rebase;
mod reflog;
mod rehearse;
mod release;
mod remerge;
mod repository_tools;
mod resolver;
mod revert_piece;
#[cfg(test)]
mod revert_tests;
mod review_workflow;
mod rewrite;
mod running;
#[cfg(test)]
mod samples;
mod saved_views;
mod sidebar;
mod slow_banner;
mod snapshot;
mod sparse;
mod ssh_setup;
mod stack;
mod state;
mod surgery;
mod symbol_biography;
mod terminals;
mod test_lane;
#[cfg(test)]
mod tests;
mod timeline;
mod toolbar;
mod tour;
mod unity;
mod versions;
mod watch;
mod watch_remote;
mod weakened_marks;
pub(crate) mod widgets;
mod working_drafts;
pub(crate) mod worktree_tab;
#[cfg(test)]
mod worktree_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use crate::forge::{Forge, Hosting, PullRequest};
use crate::git::{
    self, Commit, CommitDetail, Identity, Refs, Remote, Stash, Submodule, WorkStatus, Worktree,
};
use crate::graph::Graph;

use actions::abort_action;
pub(crate) use actions::{Action, fast_forward_action, merge_action, rebase_action};
pub use dialogs::Dialog;
pub(crate) use drafts::DraftShape;
use messages::{Msg, spawn};
pub(crate) use palette::{PaletteGeneration, PaletteItem, PaletteRun, Pick};
pub(crate) use state::{Compare, CompareData, DiffKey, DiffView, Refresh, Reword, Undo, UndoEntry};
use state::{OpKind, PendingOp, Search};
pub(crate) use test_lane::TEST_LANE_TIP;
pub(crate) use toolbar::reveal_label;
pub use widgets::primary_button;

/// Hash of the pseudo-commit that stands for uncommitted changes at the top of the graph.
pub(crate) const WIP_HASH: &str = "\u{0}wip";
/// History deeper than this isn't loaded by scrolling or searching.
pub(crate) const MAX_HISTORY: usize = 100_000;

/// How a tab asks the app to open a folder.
pub enum OpenRequest {
    NewTab(PathBuf),
    /// In the same tab or a new one, as the settings say.
    Worktree(PathBuf),
}

#[path = "tab_owner/model.rs"]
mod tab_model;
pub use tab_model::RepoTab;
#[path = "tab_owner/construction.rs"]
mod tab_construction;

/// A hash as the UI shows it: its first 8 characters.
pub(crate) fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// `&[&["checkout", "main"]]` → owned argument lists.
pub(crate) fn ops(list: &[&[&str]]) -> Vec<Vec<String>> {
    list.iter()
        .map(|args| args.iter().map(|a| (*a).to_owned()).collect())
        .collect()
}

mod prompt_session;
