//! The preview as a tab reads it: in the background, into the dialog that asked for it.

use super::*;
use crate::git::{Branch, Refs};
use crate::repo::{Action, Dialog, RepoTab, ops};

fn repo(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-preview-tab-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "second"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

fn rev(dir: &std::path::Path, rev: &str) -> String {
    git::run(Some(dir), &["rev-parse", rev])
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn hard_reset(dir: &std::path::Path) -> Dialog {
    let (head, base) = (rev(dir, "HEAD"), rev(dir, "HEAD~1"));
    Dialog::confirm(
        "Hard reset",
        "Move main?".to_owned(),
        "Reset hard",
        ops(&[&["reset", "--hard", &base]]),
        None,
    )
    .undo(Undo::HeadMove { mode: "--keep" })
    .preview(
        PreviewAsk::reset("main", &head, &base, None, true),
        "Leaving main".to_owned(),
    )
}

fn state(tab: &RepoTab) -> Option<&PreviewState> {
    match &tab.dialog {
        Some(Dialog::Confirm { preview, .. }) => Some(&preview.state),
        _ => None,
    }
}

/// The dialog opens at once, showing that it is reading; what it reads arrives later, from
/// another thread.
#[test]
fn a_confirmation_reads_what_it_changes_in_the_background() {
    let dir = repo("background");
    std::fs::write(dir.join("lost file.txt"), "x").ok();
    assert!(git::run(Some(&dir), &["add", "--", "lost file.txt"]).is_ok());
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(Action::Dialog(hard_reset(&dir)), &ctx);
    assert!(matches!(state(&tab), Some(PreviewState::Loading(_))));
    for _ in 0..400 {
        tab.poll(&ctx);
        if !matches!(state(&tab), Some(PreviewState::Loading(_))) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let found = match state(&tab) {
        Some(PreviewState::Ready(found)) => Some(found.clone()),
        _ => None,
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(found.is_some(), "the preview never arrived");
    let found = found.unwrap_or_default();
    assert_eq!(found.leaving.len(), 1);
    assert_eq!(found.leaving[0].summary, "second");
    assert_eq!(found.leaving[0].kept, Kept::OnlyHere);
    assert_eq!(found.lost_files, ["lost file.txt"]);
}

/// An answer for a dialog that has since been replaced (by another confirmation) is not shown
/// in the new one; and a confirmation that needs no git is ready at once.
#[test]
fn an_older_answer_does_not_fill_a_newer_dialog() {
    let dir = repo("stale");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(Action::Dialog(hard_reset(&dir)), &ctx);
    let first = match state(&tab) {
        Some(PreviewState::Loading(n)) => *n,
        _ => 0,
    };
    assert_ne!(first, 0, "not reading");
    tab.apply(Action::Dialog(hard_reset(&dir)), &ctx);
    tab.preview_read(first, Err("old".to_owned()));
    assert!(matches!(state(&tab), Some(PreviewState::Loading(n)) if *n != first));

    let discard = Dialog::confirm("Discard", String::new(), "Discard", Vec::new(), None)
        .preview(PreviewAsk::files(vec!["a.txt".to_owned()]), String::new());
    tab.apply(Action::Dialog(discard), &ctx);
    let ready = matches!(state(&tab), Some(PreviewState::Ready(p)) if p.lost_files == ["a.txt"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready);
}

fn branch(name: &str, upstream: &str, track: &str) -> Branch {
    Branch {
        name: name.to_owned(),
        target: "abc".to_owned(),
        is_head: false,
        upstream: upstream.to_owned(),
        track: track.to_owned(),
    }
}

/// The upstream a force push would go to: only a remote-tracking branch that is there.
#[test]
fn the_upstream_is_a_remote_branch_that_is_there() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(std::env::temp_dir(), &ctx);
    tab.refs = Refs {
        local: vec![
            branch("main", "origin/main", ""),
            branch("gone", "origin/gone", "[gone]"),
            branch("local", "", ""),
            branch("ambiguous", "remotes/origin/x", ""),
            branch("on-local", "main", ""),
        ],
        remotes: vec![(
            "origin".to_owned(),
            vec![branch("origin/main", "", ""), branch("origin/x", "", "")],
        )],
        ..Refs::default()
    };
    assert_eq!(
        tab.upstream_ref("main").as_deref(),
        Some("refs/remotes/origin/main")
    );
    assert_eq!(tab.upstream_ref("gone"), None);
    assert_eq!(tab.upstream_ref("local"), None);
    assert_eq!(tab.upstream_ref("missing"), None);
    assert_eq!(tab.upstream_ref("on-local"), None);
    assert_eq!(
        tab.upstream_ref("ambiguous").as_deref(),
        Some("refs/remotes/origin/x")
    );
}

/// The dialog as drawn: the commits and files read, the commands folded until opened, the undo
/// note; and confirming takes the action the dialog has in place of its ops.
#[test]
fn the_dialog_shows_the_preview_and_confirms_its_action() {
    use egui_kittest::kittest::Queryable;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(std::env::temp_dir(), &ctx);
    let found = Preview {
        leaving: vec![
            git::LeavingCommit {
                hash: "1111111111".to_owned(),
                summary: "fresh work".to_owned(),
                kept: Kept::OnlyHere,
            },
            git::LeavingCommit {
                hash: "2222222222".to_owned(),
                summary: "older work".to_owned(),
                kept: Kept::On("feat".to_owned()),
            },
        ],
        leaving_total: 342,
        only_here_total: 13,
        pushed: Some(git::Pushed {
            upstream: "origin/main".to_owned(),
            commits: 2,
        }),
        lost_files: vec!["src/ünï code.rs".to_owned()],
        lost_total: 1,
    };
    let quick = Action::QuickEdit {
        edit: crate::repo::quick_edit::Edit::Drop,
        hash: "1111111111".to_owned(),
    };
    let mut dialog = Dialog::confirm(
        "Drop commit",
        "Drop it?".to_owned(),
        "Drop",
        Vec::new(),
        None,
    )
    .instead(
        quick,
        vec!["git rebase -i --autostash 11111111^".to_owned()],
    )
    .undo(Undo::HeadMove { mode: "--keep" })
    .preview(PreviewAsk::nothing(), "Leaving main".to_owned());
    if let Dialog::Confirm { preview, .. } = &mut dialog {
        preview.state = PreviewState::Ready(found);
    }
    tab.dialog = Some(dialog);
    let acts = std::cell::RefCell::new(Vec::new());
    let mut harness = egui_kittest::Harness::builder()
        .with_size([700.0, 900.0])
        .build_ui(|ui| {
            // The app's fonts, which take a frame to be in.
            if ui.ctx().cumulative_pass_nr() == 0 {
                crate::theme::install_fonts(ui.ctx());
            } else {
                tab.dialogs(ui.ctx(), &mut acts.borrow_mut());
            }
        });
    harness.run();
    for text in [
        "Leaving main (342)",
        "fresh work",
        "only here",
        "still on feat",
        "and 340 more, 12 of them only here",
        "pushing it again will need a force push",
        "Uncommitted changes lost (1)",
        "src/ünï code.rs",
        "+Z moves the branch back, not the uncommitted changes.",
    ] {
        assert!(
            harness.query_all_by_label_contains(text).next().is_some(),
            "{text} isn't shown"
        );
    }
    // Folded until opened.
    assert!(
        harness
            .query_all_by_label_contains("git rebase -i")
            .next()
            .is_none()
    );
    harness.get_by_label_contains("Git commands").click();
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("git rebase -i --autostash")
            .next()
            .is_some()
    );
    harness.get_by_label("Drop").click();
    harness.run();
    drop(harness);
    let acts = acts.into_inner();
    assert!(
        matches!(acts.as_slice(), [Action::QuickEdit { hash, .. }] if hash == "1111111111"),
        "{} actions",
        acts.len()
    );
    assert!(tab.dialog.is_none());
}
