//! Source-owned contracts, assembled in their original matching order.
use super::*;
mod agent;
mod agents;
mod atlas;
mod bakeoff;
mod bisect;
mod ci_view;
mod content_recovery;
mod custom_commands;
mod dco;
mod detached;
mod devcontainer;
mod doctor;
mod email;
mod fence_view;
mod file_comparison;
mod forks;
mod guide;
mod history_activity;
mod hotfix;
mod issue_work;
mod lfs_locks;
mod lfs_storage;
mod line_dossier;
mod operation;
mod other;
mod owners;
mod parallel_bisect;
mod partial_stash;
mod postcards;
mod pr_view;
mod push;
mod reflog;
mod rehearse;
mod release;
mod repository_tools;
mod saved_views;
mod ssh_setup;
mod stack;
mod status;
mod surgery;
mod test_lane;
mod timeline;
mod tour;
mod unity;
mod versions;
mod working_drafts;

pub(super) const ROWS: &[Case] = &crate::repo::messages::workflow::ordered::<_, 332>(&[
    atlas::ROWS,
    history_activity::ROWS,
    line_dossier::ROWS,
    file_comparison::ROWS,
    ssh_setup::ROWS,
    release::ROWS,
    doctor::ROWS,
    stack::ROWS,
    forks::ROWS,
    postcards::ROWS,
    agents::ROWS,
    repository_tools::ROWS,
    timeline::ROWS,
    detached::ROWS,
    versions::ROWS,
    rehearse::ROWS,
    guide::ROWS,
    reflog::ROWS,
    surgery::ROWS,
    unity::ROWS,
    push::ROWS,
    status::ROWS,
    dco::ROWS,
    issue_work::ROWS,
    parallel_bisect::ROWS,
    pr_view::ROWS,
    lfs_locks::ROWS,
    test_lane::ROWS,
    hotfix::ROWS,
    owners::ROWS,
    operation::ROWS,
    fence_view::ROWS,
    lfs_storage::ROWS,
    devcontainer::ROWS,
    custom_commands::ROWS,
    tour::ROWS,
    ci_view::ROWS,
    other::ROWS,
    saved_views::ROWS,
    bisect::ROWS,
    content_recovery::ROWS,
    email::ROWS,
    working_drafts::ROWS,
    agent::ROWS,
    partial_stash::ROWS,
    bakeoff::ROWS,
]);
