//! What the palette offers for a repository: no entry per branch and verb, the selection's
//! entries, the second step's choices.

use std::sync::Arc;

use super::{PaletteItem, PaletteRun, Pick};
use crate::git::{Area, Branch, Commit, StatusFile};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, WIP_HASH};

fn tab() -> RepoTab {
    let ctx = egui::Context::default();
    RepoTab::new_deferred("/nonexistent/palette-test".into(), &ctx)
}

fn branch(name: &str, head: bool) -> Branch {
    Branch {
        name: name.to_owned(),
        target: "1".repeat(40),
        is_head: head,
        upstream: String::new(),
        track: String::new(),
    }
}

/// A tab on `main` with `n` other local branches, and one remote branch.
fn tab_with_branches(n: usize) -> RepoTab {
    let mut tab = tab();
    tab.refs.head = "a".repeat(40);
    tab.refs.current = Some("main".to_owned());
    tab.refs.local.push(branch("main", true));
    for k in 0..n {
        tab.refs.local.push(branch(&format!("feature/{k}"), false));
    }
    tab.refs
        .remotes
        .push(("origin".to_owned(), vec![branch("origin/main", false)]));
    tab
}

fn commit(hash: &str, parents: usize) -> Arc<Commit> {
    Arc::new(Commit {
        hash: hash.to_owned(),
        parents: vec!["0".repeat(40); parents],
        summary: "the summary".to_owned(),
        ..Commit::default()
    })
}

fn labels(items: &[PaletteItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

fn work(path: &str, code: char, area: Area) -> DiffKey {
    DiffKey::Work(StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area,
    })
}

fn file_labels(tab: &mut RepoTab, key: DiffKey) -> Vec<String> {
    tab.diff = Some(DiffView::new(key));
    tab.palette_items(&egui::Context::default())
        .into_iter()
        .filter(|i| i.selection)
        .map(|i| i.label)
        .collect()
}

// Keep fixture helpers and shard-visible test names in this module.
include!("tests/selection.rs");
include!("tests/registry.rs");
