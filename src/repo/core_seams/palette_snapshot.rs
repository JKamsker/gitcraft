//! Pins the composed palette: every entry's label, hint, shortcut, key, selection flag and
//! what it runs, in order, across a few repository contexts. The fixture was recorded before
//! feature action families moved into nested enums (#194, #198) and must not change when an
//! intent only changes where it is declared.
//!
//! Record again with `GITCRAFT_WRITE_PALETTE_SNAPSHOT=1` only for an intended palette change.
use super::*;
use crate::git::{Commit, StatusFile};
use crate::repo::{DiffKey, DiffView, PaletteItem, PaletteRun};
use std::sync::Arc;

const FIXTURE: &str = include_str!("palette_snapshot.txt");
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/repo/core_seams/palette_snapshot.txt"
);

/// What an action is, named by its intent as it was declared before the families moved.
fn moved(action: &Action) -> Option<String> {
    Some(match action {
        Action::DiffCoverage(act) => format!("DiffCoverage({act:?})"),
        Action::OpenFileComparison(page) => format!("OpenFileComparison({page:?})"),
        Action::FileView { path, rev, tab } => format!("FileView({path:?}, {rev:?}, {tab:?})"),
        Action::InteractiveRebase { onto, label } => {
            format!("InteractiveRebase({onto:?}, {label:?})")
        }
        Action::SavePatch { hash, summary } => format!("SavePatch({hash:?}, {summary:?})"),
        Action::ApplyPatch => "ApplyPatch".into(),
        Action::OpenSplit(hash) => format!("OpenSplit({hash:?})"),
        Action::OpenMove(hash) => format!("OpenMove({hash:?})"),
        Action::OpenAbsorb => "OpenAbsorb".into(),
        Action::EarlierVersions => "EarlierVersions".into(),
        _ => return None,
    })
}

fn root(action: &Action) -> String {
    match action {
        Action::Fetch => "Fetch".into(),
        Action::Pull => "Pull".into(),
        Action::Push => "Push".into(),
        Action::PullWith(mode) => format!("PullWith({mode:?})"),
        Action::Sync => "Sync".into(),
        Action::PushSkippingHooks => "PushSkippingHooks".into(),
        Action::Key(command) => format!("Key({command:?})"),
        Action::Undo => "Undo".into(),
        Action::Redo => "Redo".into(),
        Action::ToggleTimeline => "ToggleTimeline".into(),
        Action::ToggleSearch => "ToggleSearch".into(),
        Action::FocusSearch => "FocusSearch".into(),
        Action::Reload => "Reload".into(),
        Action::ToggleActivity => "ToggleActivity".into(),
        Action::DvcPull => "DvcPull".into(),
        Action::OpenBehaviour => "OpenBehaviour".into(),
        Action::OpenLinkedMachines => "OpenLinkedMachines".into(),
        Action::OpenBundleExport => "OpenBundleExport".into(),
        Action::OpenBundleInspect => "OpenBundleInspect".into(),
        Action::OpenPath(path) => format!("OpenPath({path:?})"),
        Action::OpenInEditor(path) => format!("OpenInEditor({path:?})"),
        Action::OpenUrl(url) => format!("OpenUrl({url:?})"),
        Action::SetOffline(on) => format!("SetOffline({on})"),
        Action::OpenDoctor(section) => format!("OpenDoctor({section:?})"),
        Action::OpenCi { sha, run } => format!("OpenCi({sha:?}, {run:?})"),
        Action::Checkout(name) => format!("Checkout({name:?})"),
        Action::CheckoutRemote(name) => format!("CheckoutRemote({name:?})"),
        Action::Ops { label, ops, .. } => format!("Ops({label:?}, {ops:?})"),
        Action::UndoableOps { label, ops, .. } => format!("UndoableOps({label:?}, {ops:?})"),
        Action::Popout(request) => format!("Popout({request:?})"),
        Action::Rehearse(act) => format!("Rehearse({act:?})"),
        Action::Rerere(act) => format!("Rerere({act:?})"),
        Action::Decisions(act) => format!("Decisions({act:?})"),
        Action::Unity(act) => format!("Unity({act:?})"),
        Action::LfsLocks(act) => format!("LfsLocks({act:?})"),
        Action::LfsStorage(act) => format!("LfsStorage({act:?})"),
        Action::TestLane(act) => format!("TestLane({act:?})"),
        Action::Dialog(_) => "Dialog(..)".into(),
        Action::Reflog(_) => "Reflog(..)".into(),
        Action::Terminal(_) => "Terminal(..)".into(),
        Action::Cleanup(_) => "Cleanup(..)".into(),
        Action::RepositoryTools(_) => "RepositoryTools(..)".into(),
        Action::Agent(_) => "Agent(..)".into(),
        Action::Bake(_) => "Bake(..)".into(),
        Action::Package(_) => "Package(..)".into(),
        Action::Versions(_) => "Versions(..)".into(),
        Action::Insights(_) => "Insights(..)".into(),
        Action::Owners(_) => "Owners(..)".into(),
        Action::Fork(_) => "Fork(..)".into(),
        Action::Issue(_) => "Issue(..)".into(),
        Action::Hotfix(_) => "Hotfix(..)".into(),
        Action::Release(_) => "Release(..)".into(),
        Action::Bisect(_) => "Bisect(..)".into(),
        Action::ParallelBisect(_) => "ParallelBisect(..)".into(),
        Action::Custom(_) => "Custom(..)".into(),
        Action::LocalCi(_) => "LocalCi(..)".into(),
        Action::Email(_) => "Email(..)".into(),
        Action::Devcontainer(_) => "Devcontainer(..)".into(),
        Action::Stash(_) => "Stash(..)".into(),
        Action::Timeline(_) => "Timeline(..)".into(),
        Action::Compare { base, target, .. } => format!("Compare({base:?}, {target:?})"),
        _ => "UNDESCRIBED".into(),
    }
}

fn describe(run: &PaletteRun) -> String {
    match run {
        PaletteRun::Pick(pick) => format!("Pick({pick:?})"),
        PaletteRun::Action(action) => moved(action).unwrap_or_else(|| root(action)),
    }
}

fn line(context: &str, item: &PaletteItem) -> String {
    format!(
        "{context}\t{}\t{}\t{}\t{}\t{}\t{}",
        u8::from(item.selection),
        item.label,
        item.hint,
        item.shortcut.as_deref().unwrap_or("-"),
        item.key,
        describe(&item.run)
    )
}

fn commit(hash: String, parents: Vec<String>, summary: &str) -> Arc<Commit> {
    Arc::new(Commit {
        hash,
        parents,
        summary: summary.into(),
        ..Default::default()
    })
}

/// One repository tab per context: none of them reads a repository.
fn context(name: &str, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/palette-snapshot".into(), ctx);
    let head = "a".repeat(40);
    let older = "b".repeat(40);
    tab.refs.head = head.clone();
    tab.refs.current = Some("main".into());
    tab.commits = vec![
        commit(head.clone(), vec![older.clone()], "recorded head"),
        commit(older.clone(), Vec::new(), "recorded parent"),
    ];
    match name {
        "empty" => {
            tab.refs.head.clear();
            tab.refs.current = None;
            tab.commits.clear();
        }
        "head-commit-file" => {
            tab.gerrit = Some(crate::gerrit::Server {
                remote: "origin".into(),
                web: "https://review.example.invalid".into(),
                project: "project".into(),
                web_warning: None,
            });
            tab.selected = Some(0);
            tab.busy_op = Some("recorded operation".into());
            tab.lfs = true;
            tab.diff = Some(DiffView::new(DiffKey::Commit {
                hash: head,
                path: "src/lib.rs".into(),
            }));
        }
        "older-work-file" => {
            tab.selected = Some(1);
            tab.lfs = true;
            tab.diff = Some(DiffView::new(DiffKey::Work(StatusFile {
                path: "README.md".into(),
                old_path: None,
                code: 'M',
                area: crate::git::Area::Unstaged,
            })));
        }
        _ => {}
    }
    tab
}

const CONTEXTS: [&str; 4] = ["empty", "idle", "head-commit-file", "older-work-file"];

fn snapshot() -> Vec<String> {
    let ctx = egui::Context::default();
    let mut lines = Vec::new();
    for name in CONTEXTS {
        let tab = context(name, &ctx);
        lines.extend(tab.palette_items(&ctx).iter().map(|item| line(name, item)));
    }
    lines
}

#[test]
fn composed_palette_matches_the_recorded_snapshot() {
    let lines = snapshot();
    let undescribed: Vec<_> = lines
        .iter()
        .filter(|l| l.ends_with("UNDESCRIBED"))
        .collect();
    assert!(
        undescribed.is_empty(),
        "describe these intents: {undescribed:#?}"
    );
    if std::env::var_os("GITCRAFT_WRITE_PALETTE_SNAPSHOT").is_some() {
        let mut text = lines.join("\n");
        text.push('\n');
        std::fs::write(FIXTURE_PATH, text).expect("write the palette snapshot");
        return;
    }
    let recorded: Vec<_> = FIXTURE.lines().collect();
    assert_eq!(lines.len(), recorded.len(), "palette entry count changed");
    for (index, (now, then)) in lines.iter().zip(&recorded).enumerate() {
        assert_eq!(now, then, "palette entry {index} changed");
    }
}
