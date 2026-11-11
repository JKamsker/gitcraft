use crate::git;
use crate::repo::OpKind;
use crate::repo::commit_actions::reset_steps;
use crate::repo::operations::run_steps;
use egui_kittest::kittest::Queryable;

fn area_d_tab() -> crate::repo::RepoTab {
    use std::sync::Arc;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/area-d".into(), &ctx);
    tab.refs.head = "b".repeat(40);
    tab.refs.current = Some("main".to_owned());
    tab.commits = vec![
        Arc::new(git::Commit {
            hash: crate::repo::WIP_HASH.into(),
            ..Default::default()
        }),
        Arc::new(git::Commit {
            hash: "a".repeat(40),
            summary: "A selected commit".into(),
            ..Default::default()
        }),
    ];
    tab.selected = Some(0);
    tab
}

#[test]
fn area_d_working_tree_comparison_is_unique_and_preserves_scope() {
    let mut h = egui_kittest::Harness::builder().build_ui_state(
        |ui, (tab, acts): &mut (crate::repo::RepoTab, Vec<crate::repo::Action>)| {
            tab.compare_with_items(ui, 1, &"a".repeat(40), "aaaaaaaa", acts);
        },
        (area_d_tab(), Vec::new()),
    );
    h.run();
    assert_eq!(
        h.query_all_by_label("Compare with the working tree")
            .count(),
        1
    );
    h.get_by_label("Compare with the working tree").click();
    h.run();
    assert!(
        matches!(h.state().1.first(), Some(crate::repo::Action::Compare { base, target: None, .. }) if base == &"a".repeat(40))
    );
    h.state_mut().0.selected = Some(1);
    h.run();
    assert_eq!(
        h.query_all_by_label("Compare with the working tree")
            .count(),
        1
    );
}

/// Both root and submenu flip into the screen at the lower right. Their actions retain
/// the clicked row while the selected row is WIP (the S14 duplication trigger).
#[test]
fn area_d_menu_matrix_repositions_and_keeps_submenus_reachable() -> Result<(), String> {
    for width in [640.0, 1100.0, 1600.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = egui_kittest::Harness::builder()
                .with_size([width / scale, 760.0 / scale])
                .with_pixels_per_point(scale)
                .build_ui_state(
                    |ui,
                     (tab, acts, fonts): &mut (
                        crate::repo::RepoTab,
                        Vec<crate::repo::Action>,
                        bool,
                    )| {
                        if !*fonts {
                            crate::theme::install_fonts(ui.ctx());
                            *fonts = true;
                            return;
                        }
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                            let row = ui.button("Commit row");
                            ui.data_mut(|d| {
                                d.insert_temp(
                                    super::pill_menu_id(&"a".repeat(40)),
                                    Some("fork/a-long-feature-branch".to_owned()),
                                )
                            });
                            tab.commit_menu(&row, 1, true, acts);
                        });
                    },
                    (area_d_tab(), Vec::new(), false),
                );
            h.run();
            h.get_by_label("Commit row").click_secondary();
            h.run();
            h.event(egui::Event::PointerMoved(egui::pos2(2.0, 2.0)));
            h.run_steps(12);
            let root = h.get_by_label_contains("Links / Copy").rect();
            assert!(
                h.ctx.content_rect().contains_rect(root),
                "root outside viewport {width}/{scale}: {root:?}"
            );
            if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                h.render()?
                    .save(format!("{dir}/menu-{width:.0}-{scale:.2}.png"))
                    .map_err(|e| e.to_string())?;
            }
            h.get_by_label("Compare ⏵").hover();
            h.run_steps(12);
            let compare = h.get_by_label("Compare with the working tree").rect();
            let sibling = h.get_by_label_contains("Rewrite").rect();
            assert!(
                !compare.intersects(sibling),
                "submenu covers sibling hover target {width}/{scale}: {compare:?} / {sibling:?}"
            );
            assert!(
                h.ctx.content_rect().contains_rect(compare),
                "submenu outside viewport {width}/{scale}: {compare:?}"
            );
            if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                h.render()?
                    .save(format!("{dir}/compare-{width:.0}-{scale:.2}.png"))
                    .map_err(|e| e.to_string())?;
            }
            h.get_by_label("Compare with the working tree").click();
            h.run();
            assert!(
                matches!(h.state().1.first(), Some(crate::repo::Action::Compare { base, target: None, .. }) if base == &"a".repeat(40))
            );
            assert!(!egui::Popup::is_any_open(&h.ctx));
            h.get_by_label("Commit row").click_secondary();
            h.run();
            h.get_by_label("Compare ⏵").focus();
            h.key_press(egui::Key::Enter);
            h.run();
            h.get_by_label("Compare with the working tree");
            h.key_press(egui::Key::Escape);
            h.run();
            assert!(!egui::Popup::is_any_open(&h.ctx));
        }
    }
    Ok(())
}

/// Ref reloads may insert/reorder siblings while the pointer rests on an action in
/// an open submenu. Its button identity must belong to the branch, not its row index.
#[test]
fn area_d_followup_branch_submenu_keeps_its_target_across_ref_reordering() {
    let branch = |name: &str| git::Branch {
        name: name.to_owned(),
        target: "a".repeat(40),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    };
    for updated in [
        vec!["alpha", "bravo", "charlie"],
        vec!["charlie", "bravo"],
        vec!["bravo"],
    ] {
        let mut tab = area_d_tab();
        tab.refs.local = vec![branch("bravo"), branch("charlie")];
        let mut h = egui_kittest::Harness::builder()
            .with_size([1000.0, 800.0])
            .build_ui_state(
                |ui, (tab, acts): &mut (crate::repo::RepoTab, Vec<crate::repo::Action>)| {
                    let r = ui.button("Commit row");
                    tab.commit_menu(&r, 1, true, acts);
                },
                (tab, Vec::new()),
            );
        h.run();
        h.get_by_label("Commit row").click_secondary();
        h.run();
        h.get_by_label("Branches / Checkout ⏵").hover();
        h.run_steps(12);
        h.get_by_label("Branch bravo").hover();
        h.run_steps(12);
        h.get_by_label("Check out").hover();
        h.run_steps(12);
        h.state_mut().0.refs.local = updated.iter().map(|name| branch(name)).collect();
        h.run_steps(12);
        h.get_by_label("Check out").click();
        h.run();
        let targets: Vec<_> = h
            .state()
            .1
            .iter()
            .filter_map(|action| match action {
                crate::repo::Action::Checkout(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            targets,
            ["bravo"],
            "open submenu changed scope after refs became {updated:?}"
        );
    }
}

/// A hard reset asked for on `main` does nothing once another branch is checked out (in the
/// terminal, while the dialog was open): it would throw away that one's uncommitted work.
#[test]
fn a_reset_refuses_once_another_branch_is_checked_out() {
    let dir = std::env::temp_dir().join(format!("gitcraft-reset-moved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git(args).is_ok(), "{args:?}");
    }
    let base = git::head(&dir);
    assert!(git(&["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
    let seen = git::head(&dir);
    let steps = reset_steps("--hard", &base, "main", &seen);
    // Then, elsewhere: another branch, with a commit and work not committed.
    for args in [
        &["switch", "-q", "-c", "other"][..],
        &["commit", "-q", "--allow-empty", "-m", "other work"],
    ] {
        assert!(git(args).is_ok(), "{args:?}");
    }
    let other = git::head(&dir);
    let _ = std::fs::write(dir.join("wip.txt"), "unsaved\n");
    assert!(git(&["add", "wip.txt"]).is_ok());
    let (refused, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let kept = (git::head(&dir), dir.join("wip.txt").exists());
    // Back on main as the dialog saw it, the reset goes through.
    let _ = git(&["reset", "-q", "--hard"]);
    let back = git(&["switch", "-q", "main"]);
    let (reset, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let now = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_err());
    assert_eq!(kept, (other, true));
    assert!(back.is_ok() && reset.is_ok());
    assert_eq!(now, base);
}

/// Nor once another branch at the same commit is checked out: it would move that branch.
/// And one asked for on a detached HEAD refuses once a branch is checked out there.
#[test]
fn a_reset_refuses_once_a_branch_at_the_same_commit_is_checked_out() {
    let dir = std::env::temp_dir().join(format!("gitcraft-reset-same-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git(args).is_ok(), "{args:?}");
    }
    let base = git::head(&dir);
    assert!(git(&["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
    let seen = git::head(&dir);
    let steps = reset_steps("--hard", &base, "main", &seen);
    let detached = reset_steps("--hard", &base, "HEAD", &seen);
    // `feature` is where `main` is, and gets checked out while the dialog is open.
    let switched = git(&["switch", "-q", "-c", "feature"]);
    let (refused, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let (refused_detached, _) = run_steps(&dir, &detached, OpKind::plain(), None);
    let feature = git::resolve(&dir, "refs/heads/feature");
    // Back on main, it goes through; detached, so does the other.
    let back = git(&["switch", "-q", "main"]);
    let (reset, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let main = git::resolve(&dir, "refs/heads/main");
    let detach = git(&["switch", "-q", "--detach", "feature"]);
    let (reset_detached, _) = run_steps(&dir, &detached, OpKind::plain(), None);
    let (head, feature_after) = (git::head(&dir), git::resolve(&dir, "refs/heads/feature"));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(switched.is_ok() && back.is_ok() && detach.is_ok());
    assert!(refused.is_err());
    assert!(refused_detached.is_err());
    assert_eq!(feature, Ok(seen.clone()));
    assert!(reset.is_ok(), "{reset:?}");
    assert_eq!(main, Ok(base.clone()));
    assert!(reset_detached.is_ok(), "{reset_detached:?}");
    assert_eq!(head, base);
    assert_eq!(feature_after, Ok(seen));
}

/// A repository at `dir` with `ünï cödé.txt` (a space and accents in its name) changed
/// between `base` and `two`, and `other file.txt` left alone. The hashes of both.
fn keep_repo(name: &str) -> (std::path::PathBuf, String, String) {
    let dir = std::env::temp_dir().join(format!("gitcraft-keep-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        // The files read back as written, whatever Windows' git would convert.
        &["config", "core.autocrlf", "false"],
    ] {
        assert!(git(args).is_ok(), "{args:?}");
    }
    let _ = std::fs::write(dir.join("ünï cödé.txt"), "one\n");
    let _ = std::fs::write(dir.join("other file.txt"), "same\n");
    assert!(git(&["add", "-A"]).is_ok());
    assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
    let base = git::head(&dir);
    let _ = std::fs::write(dir.join("ünï cödé.txt"), "two\n");
    assert!(git(&["commit", "-q", "-am", "two"]).is_ok());
    let two = git::head(&dir);
    (dir, base, two)
}

/// `reset --keep` moves the branch and keeps a change to a file the move doesn't touch.
#[test]
fn a_keep_reset_moves_the_branch_and_keeps_local_changes() {
    let (dir, base, two) = keep_repo("ok");
    let _ = std::fs::write(dir.join("other file.txt"), "mine, not committed\n");
    let steps = reset_steps("--keep", &base, "main", &two);
    let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let head = git::head(&dir);
    let kept = std::fs::read_to_string(dir.join("other file.txt")).unwrap_or_default();
    let moved = std::fs::read_to_string(dir.join("ünï cödé.txt")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(head, base);
    assert_eq!(kept, "mine, not committed\n");
    assert_eq!(moved, "one\n");
}

/// When the move would touch a file with changes, it stops with git's own message and
/// changes nothing: the branch stays, the change stays.
#[test]
fn a_keep_reset_refuses_to_touch_a_changed_file() {
    let (dir, base, two) = keep_repo("refused");
    let _ = std::fs::write(dir.join("ünï cödé.txt"), "mine, not committed\n");
    let steps = reset_steps("--keep", &base, "main", &two);
    let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let head = git::head(&dir);
    let kept = std::fs::read_to_string(dir.join("ünï cödé.txt")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let err = result.err().unwrap_or_default();
    assert!(
        err.contains("ünï cödé.txt") || err.contains("not uptodate"),
        "{err}"
    );
    assert_eq!(head, two);
    assert_eq!(kept, "mine, not committed\n");
}
