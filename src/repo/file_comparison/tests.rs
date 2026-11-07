use super::*;
use crate::git::file_comparison::tests::Repo;
use crate::repo::diff::{Kind, Pick};
use egui_kittest::{Harness, kittest::Queryable};

fn read(r: &Repo, path: &str) -> Result<model::LensData, String> {
    model::prepare_lens(r.lens(path)?, &|| true)
}
fn changed(part: &model::Boundary, text: &str) -> BTreeSet<usize> {
    part.lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            (matches!(l.kind, Kind::Add | Kind::Del) && l.raw.get(1..) == Some(text)).then_some(i)
        })
        .collect()
}

#[test]
fn three_way_alignment_marks_working_edits_undoing_staged_changes_and_preserves_lf()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "one\nbase\nlast")?;
    r.commit()?;
    r.write("a", "one\nstaged\nlast")?;
    r.git(&["add", "a"])?;
    r.write("a", "one\nbase\nlast")?;
    let lens = read(&r, "a")?;
    assert!(lens.undoing_staged);
    assert_eq!(lens.rows.len(), 3);
    assert!(lens.rows[1].undoes);
    assert_eq!(
        lens.rows[1].text,
        [
            Some("base\n".into()),
            Some("staged\n".into()),
            Some("base\n".into())
        ]
    );
    assert_eq!(lens.rows[1].changes[0].len(), 2);
    assert_eq!(lens.rows[1].changes[1].len(), 2);
    assert_eq!(lens.rows[2].text[0], Some("last".into()));
    let patch = lens.boundaries[1]
        .patch("a", Pick::Stage, &lens.rows[1].changes[1])
        .ok_or("patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one\nbase\nlast");
    Ok(())
}
#[test]
fn aligned_insertions_and_deletions_do_not_shift_unrelated_index_lines() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "start\nremoved\nend\n")?;
    r.commit()?;
    r.write("a", "start\nend\n")?;
    r.git(&["add", "a"])?;
    r.write("a", "start\nadded\nend\n")?;
    let lens = read(&r, "a")?;
    assert!(
        lens.rows
            .iter()
            .any(|r| r.text == [Some("removed\n".into()), None, Some("added\n".into())])
    );
    assert!(lens.rows.iter().any(|r| r.text
        == [
            Some("end\n".into()),
            Some("end\n".into()),
            Some("end\n".into())
        ]));
    Ok(())
}
#[test]
fn partly_staged_new_files_can_stage_and_unstage_selected_lines_without_touching_working_copy()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("new", "one\ntwo\n")?;
    let lens = read(&r, "new")?;
    let p = &lens.boundaries[1];
    let chosen = changed(p, "one");
    let patch = p.patch("new", Pick::Stage, &chosen).ok_or("new patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "one");
    assert_eq!(
        std::fs::read(r.0.join("new")).map_err(|e| e.to_string())?,
        b"one\ntwo\n"
    );
    r.git(&["add", "new"])?;
    let lens = read(&r, "new")?;
    let p = &lens.boundaries[0];
    let patch = p
        .patch("new", Pick::Unstage, &changed(p, "one"))
        .ok_or("unstage patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, true)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "two");
    Ok(())
}
#[test]
fn partial_deletions_and_reversed_deleted_file_creation_are_safe() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "one\ntwo\n")?;
    r.commit()?;
    std::fs::remove_file(r.0.join("a")).map_err(|e| e.to_string())?;
    let lens = read(&r, "a")?;
    let p = &lens.boundaries[1];
    let patch = p
        .patch("a", Pick::Stage, &changed(p, "one"))
        .ok_or("delete patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "two");
    assert!(!r.0.join("a").exists());
    r.git(&["rm", "--cached", "-q", "a"])?;
    let lens = read(&r, "a")?;
    let p = &lens.boundaries[0];
    let patch = p
        .patch("a", Pick::Unstage, &changed(p, "one"))
        .ok_or("restore partial")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, true)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one");
    assert!(!r.0.join("a").exists());
    Ok(())
}
#[test]
fn quoted_git_names_and_crlf_no_newline_patches_do_not_run_programs_or_change_other_files()
-> Result<(), String> {
    let r = Repo::new()?;
    let path = if cfg!(windows) {
        "space ' $(touch INJECTED); quoted ü.txt"
    } else {
        "space ' $(touch INJECTED); \"quoted\"\tline\n.txt"
    };
    r.write(path, "old\r\nlast")?;
    r.commit()?;
    r.write(path, "new\r\nlast")?;
    let lens = read(&r, path)?;
    let p = &lens.boundaries[1];
    files::apply_lens_patch(&r.0, &lens.snapshot, &p.full, false)?;
    assert_eq!(r.git(&["show", &format!(":0:{path}")])?, "new\r\nlast");
    assert!(!r.0.join("INJECTED").exists());
    Ok(())
}

fn harness(r: &Repo, page: Page) -> Result<Harness<'static, (RepoTab, bool)>, String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.work = crate::git::status(&r.0)?;
    tab.refs = crate::git::refs(&r.0)?;
    tab.open_file_comparison(page, &ctx);
    Ok(Harness::builder()
        .with_size(egui::vec2(1300., 950.))
        .build_ui_state(
            |ui, state: &mut (RepoTab, bool)| {
                if !state.1 {
                    crate::theme::install_fonts(ui.ctx());
                    state.1 = true;
                }
                state.0.poll(ui.ctx());
                let mut acts = Vec::new();
                state.0.file_comparison_view(ui, &mut acts);
                for action in acts {
                    state.0.apply(action, ui.ctx());
                }
            },
            (tab, false),
        ))
}
fn settle(h: &mut Harness<'_, (RepoTab, bool)>) -> Result<(), String> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        h.run_steps(1);
        if h.state().0.busy_op.is_none()
            && h.state()
                .0
                .file_comparison
                .as_ref()
                .is_none_or(|s| !s.loading)
        {
            break;
        }
        if std::time::Instant::now() > until {
            return Err("file view did not settle".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if let Some(state) = &h.state().0.file_comparison {
        if let Some(Err(e)) = &state.data {
            return Err(e.clone());
        }
        if let Some(Err(e)) = &state.pair {
            return Err(e.clone());
        }
    }
    Ok(())
}
#[test]
fn actual_lens_boundary_buttons_stage_and_unstage_the_displayed_snapshot() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "one\nbase\nlast\n")?;
    r.commit()?;
    r.write("a", "one\nstaged\nlast\n")?;
    r.git(&["add", "a"])?;
    r.write("a", "one\nbase\nlast\n")?;
    let mut h = harness(&r, Page::Lens)?;
    settle(&mut h)?;
    assert!(
        h.query_by_label_contains("Working copy matches HEAD")
            .is_some()
    );
    h.get_by_label("Unstage lens row 2").click();
    settle(&mut h)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one\nbase\nlast");
    r.write("a", "one\nworking\nlast\n")?;
    h.get_by_label("Refresh file view").click();
    settle(&mut h)?;
    h.get_by_label("Stage lens row 2").click();
    settle(&mut h)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "one\nworking\nlast");
    assert_eq!(
        std::fs::read(r.0.join("a")).map_err(|e| e.to_string())?,
        b"one\nworking\nlast\n"
    );
    Ok(())
}
#[test]
fn actual_matrix_cells_open_a_read_only_content_diff_from_pinned_branches() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("config", "old\n")?;
    r.write("other", "other\n")?;
    r.commit()?;
    r.git(&["branch", "release-a"])?;
    r.write("config", "new\n")?;
    r.commit()?;
    r.git(&["branch", "release-b"])?;
    let mut h = harness(&r, Page::Matrix)?;
    settle(&mut h)?;
    {
        let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
        state.selected = ["refs/heads/release-a".into(), "refs/heads/release-b".into()]
            .into_iter()
            .collect();
    }
    h.get_by_label("Compare branch versions").click();
    settle(&mut h)?;
    h.query_by_label_contains("config | refs/heads/release-a |")
        .ok_or("first cell")?
        .click();
    h.run_steps(2);
    h.query_by_label_contains("config | refs/heads/release-b |")
        .ok_or("second cell")?
        .click();
    settle(&mut h)?;
    assert!(h.query_by_label_contains("Content edits only").is_some());
    assert!(h.query_by_label_contains("-old").is_some());
    assert!(h.query_by_label_contains("+new").is_some());
    assert!(h.query_by_label_contains("Stage lens row").is_none());
    let epoch = h
        .state()
        .0
        .file_comparison
        .as_ref()
        .ok_or("state")?
        .data_request;
    let old_request = h.state().0.file_comparison.as_ref().ok_or("state")?.request;
    h.query_by_label_contains("config | refs/heads/release-b |")
        .ok_or("deselect")?
        .click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Content edits only").is_none());
    {
        let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
        assert_eq!(state.cells.len(), 1);
        assert_eq!(state.data_request, epoch);
        assert!(!state.loading && state.pair.is_none());
        state.accept_pair(old_request, Err("late pair".into()));
        assert!(state.pair.is_none());
    }
    h.query_by_label_contains("config | refs/heads/release-a |")
        .ok_or("clear final")?
        .click();
    h.run_steps(2);
    assert!(
        h.state()
            .0
            .file_comparison
            .as_ref()
            .is_some_and(|s| s.cells.is_empty() && !s.loading && s.pair.is_none())
    );
    for label in [
        "config | refs/heads/release-a |",
        "config | refs/heads/release-b |",
    ] {
        h.query_by_label_contains(label).ok_or("pair cell")?.click();
        h.run_steps(2);
    }
    settle(&mut h)?;
    let old = h.state().0.file_comparison.as_ref().ok_or("state")?.request;
    h.query_by_label_contains("other | refs/heads/release-a |")
        .ok_or("new first")?
        .click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Content edits only").is_none());
    let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
    assert_eq!(state.cells.len(), 1);
    assert!(!state.loading && state.pair.is_none());
    state.accept_pair(old, Err("late prior selection".into()));
    assert!(state.pair.is_none());
    // Deliver a deterministically outstanding pair after clearing a selection.
    let stale = state.begin(false);
    let ctx = h.ctx.clone();
    h.state_mut()
        .0
        .comparison_cell(epoch, b"other".to_vec(), 0, &ctx);
    let state = h.state_mut().0.file_comparison.as_mut().ok_or("state")?;
    state.accept_pair(stale, Err("late pending pair".into()));
    assert!(state.pair.is_none() && !state.loading && state.cells.is_empty());
    Ok(())
}
#[test]
fn actual_migration_ribbon_opens_content_edits_apart_from_the_rename() -> Result<(), String> {
    let r = Repo::new()?;
    r.write(
        "old/a",
        (0..30).map(|i| format!("line{i}\n")).collect::<String>(),
    )?;
    r.commit()?;
    r.git(&["mv", "old/a", "new-a"])?;
    r.write(
        "new-a",
        (0..30)
            .map(|i| format!("line{}\n", if i == 10 { 99 } else { i }))
            .collect::<String>(),
    )?;
    r.git(&["add", "-A"])?;
    let mut h = harness(&r, Page::Migration)?;
    settle(&mut h)?;
    h.query_by_label_contains("old/a → new-a · Rename")
        .ok_or("ribbon")?
        .click();
    settle(&mut h)?;
    assert!(h.query_by_label_contains("Content edits only").is_some());
    assert!(h.query_by_label_contains("+line99").is_some());
    assert!(h.query_by_label_contains("rename from").is_none());
    Ok(())
}
#[test]
fn closed_refreshed_or_reopened_views_cannot_apply_old_line_actions() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "base\n")?;
    r.commit()?;
    r.write("a", "working\n")?;
    let mut h = harness(&r, Page::Lens)?;
    settle(&mut h)?;
    let (old, lines) = {
        let s = h.state().0.file_comparison.as_ref().ok_or("state")?;
        let Some(Ok(d)) = &s.data else {
            return Err("data".into());
        };
        let Data::Lens(l) = d.as_ref() else {
            return Err("lens".into());
        };
        (s.data_request, l.rows[0].changes[1].clone())
    };
    h.get_by_label("Refresh file view").click();
    settle(&mut h)?;
    let ctx = h.ctx.clone();
    h.state_mut()
        .0
        .comparison_pick(old, 1, lines.clone(), false, &ctx);
    assert!(h.state().0.busy_op.is_none());
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    h.state_mut().0.file_comparison = None;
    h.state_mut().0.open_file_comparison(Page::Lens, &ctx);
    settle(&mut h)?;
    h.state_mut().0.comparison_pick(old, 1, lines, false, &ctx);
    assert!(h.state().0.busy_op.is_none());
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    Ok(())
}
#[test]
fn opening_snapshot_views_drops_diff_reading_modes_and_selections() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "raw\n")?;
    r.commit()?;
    r.write("a", "edited\n")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.work = crate::git::status(&r.0)?;
    let file = tab.work.unstaged.first().cloned().ok_or("file")?;
    let mut view = super::super::DiffView::new(DiffKey::Work(file));
    view.structural.enabled = true;
    view.chosen.insert(1);
    tab.diff = Some(view);
    tab.diff_as_file = true;
    tab.diff_rendered = true;
    tab.open_file_comparison(Page::Lens, &ctx);
    assert!(tab.diff.is_none());
    assert!(tab.file_comparison.is_some());
    Ok(())
}

#[test]
fn malformed_line_ids_and_submodule_boundaries_are_read_only() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "base\n")?;
    r.commit()?;
    r.write("a", "edit\n")?;
    let mut h = harness(&r, Page::Lens)?;
    settle(&mut h)?;
    let request = h
        .state()
        .0
        .file_comparison
        .as_ref()
        .ok_or("state")?
        .data_request;
    let ctx = h.ctx.clone();
    h.state_mut()
        .0
        .comparison_pick(request, 1, [usize::MAX].into(), false, &ctx);
    assert!(h.state().0.busy_op.is_none());
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    let mut part = read(&r, "a")?.boundaries[1].clone();
    part.old_mode = Some("160000".into());
    assert!(!part.writable());
    assert!(part.patch("a", Pick::Stage, &part.changed()).is_none());
    let mut snapshot = r.lens("a")?;
    snapshot.versions[0].mode = Some("160000".into());
    assert!(
        files::apply_lens_patch(&r.0, &snapshot, &part.full, false)
            .is_err_and(|e| e.contains("read-only"))
    );
    assert_eq!(r.git(&["show", ":0:a"])?, "base");
    Ok(())
}

#[test]
fn intent_to_add_and_empty_file_creation_deletion_keep_commit_semantics() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("tracked", "base\n")?;
    r.commit()?;
    r.write("new", "one\ntwo\n")?;
    r.git(&["add", "-N", "new"])?;
    let lens = read(&r, "new")?;
    assert!(lens.snapshot.versions[0].mode.is_none());
    assert!(lens.snapshot.versions[1].mode.is_none());
    let p = &lens.boundaries[1];
    let patch = p
        .patch("new", Pick::Stage, &changed(p, "one"))
        .ok_or("intent-to-add patch")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "one");
    assert_eq!(
        std::fs::read(r.0.join("new")).map_err(|e| e.to_string())?,
        b"one\ntwo\n"
    );
    r.write("empty", b"")?;
    let lens = read(&r, "empty")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(r.git(&["ls-files", "--error-unmatch", "empty"]).is_ok());
    r.commit()?;
    std::fs::remove_file(r.0.join("empty")).map_err(|e| e.to_string())?;
    let lens = read(&r, "empty")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(r.git(&["ls-files", "--error-unmatch", "empty"]).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn mode_only_boundaries_and_staged_rename_content_keep_paths_and_working_bytes()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let r = Repo::new()?;
    r.write("old", "base\n")?;
    r.commit()?;
    std::fs::set_permissions(r.0.join("old"), std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let lens = read(&r, "old")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert!(
        r.git(&["ls-files", "--stage", "old"])?
            .starts_with("100755")
    );
    r.git(&["mv", "old", "new"])?;
    r.write("new", "staged\n")?;
    r.git(&["add", "new"])?;
    r.write("new", "working\n")?;
    let snapshot = files::read_lens(
        &r.0,
        &files::LensSpec {
            path: "new".into(),
            previous: Some("old".into()),
        },
        &|| true,
    )?;
    let lens = model::prepare_lens(snapshot, &|| true)?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[0].full, true)?;
    assert_eq!(r.git(&["show", ":0:new"])?, "base");
    assert!(r.git(&["ls-files", "--error-unmatch", "old"]).is_err());
    assert_eq!(
        std::fs::read(r.0.join("new")).map_err(|e| e.to_string())?,
        b"working\n"
    );
    Ok(())
}

#[test]
fn emulated_symlinks_keep_the_index_type_like_git_add_and_guard_config_changes()
-> Result<(), String> {
    let r = Repo::new()?;
    r.git(&["config", "core.symlinks", "false"])?;
    r.write("link", b"original-target")?;
    let original = r.git(&["hash-object", "--no-filters", "-w", "link"])?;
    r.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("120000,{original},link"),
    ])?;
    r.git(&["commit", "-qm", "emulated link"])?;
    let unchanged = read(&r, "link")?;
    assert_eq!(unchanged.snapshot.work_raw.mode.as_deref(), Some("100644"));
    assert!(
        unchanged
            .snapshot
            .versions
            .iter()
            .all(|v| v.mode.as_deref() == Some("120000"))
    );
    assert!(unchanged.boundaries[1].changed().is_empty());
    assert_eq!(
        unchanged.boundaries[1].old_mode,
        unchanged.boundaries[1].new_mode
    );
    r.write("link", b"new-target")?;
    r.git(&["add", "link"])?;
    let native_result = r.git(&["ls-files", "--stage", "link"])?;
    assert!(native_result.starts_with("120000"));
    r.git(&[
        "update-index",
        "--cacheinfo",
        &format!("120000,{original},link"),
    ])?;
    let lens = read(&r, "link")?;
    files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)?;
    assert_eq!(r.git(&["ls-files", "--stage", "link"])?, native_result);
    assert_eq!(
        std::fs::read(r.0.join("link")).map_err(|e| e.to_string())?,
        b"new-target"
    );
    r.write("link", b"later-target")?;
    let lens = read(&r, "link")?;
    r.git(&["config", "core.symlinks", "true"])?;
    assert!(
        files::apply_lens_patch(&r.0, &lens.snapshot, &lens.boundaries[1].full, false)
            .is_err_and(|e| e.contains("stale"))
    );
    assert_eq!(r.git(&["ls-files", "--stage", "link"])?, native_result);
    Ok(())
}

#[test]
fn actual_cross_file_move_opens_both_places_and_navigates_after_the_patch_arrives()
-> Result<(), String> {
    let r = Repo::new()?;
    let content = "fn important_validation_function(value: i32) {\n    validate_input(value);\n    record_the_validation_result(value);\n}\n";
    r.write("source.rs", content)?;
    r.commit()?;
    r.write("source.rs", "")?;
    r.write("destination.rs", content)?;
    r.git(&["add", "."])?;
    let mut h = harness(&r, Page::CodeMoves)?;
    settle(&mut h)?;
    h.get_by_label("source.rs:1 → destination.rs:1").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("M1 · Moved to").is_some());
    h.get_by_label("Open destination destination.rs:1").click();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        h.run_steps(1);
        if h.state().0.diff.as_ref().is_some_and(|v| v.data.is_some()) {
            break;
        }
        if std::time::Instant::now() > until {
            return Err("Destination patch did not arrive".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().0.file_comparison.is_none());
    let view = h.state().0.diff.as_ref().ok_or("destination")?;
    assert_eq!(view.key.path(), "destination.rs");
    let data = view
        .data
        .as_ref()
        .ok_or("patch")?
        .as_ref()
        .map_err(Clone::clone)?;
    assert_eq!(
        data.lines[view.jump_line.ok_or("line target")?].new,
        Some(1)
    );
    Ok(())
}

#[test]
fn line_history_header_wrap_toggles_without_reloading_or_losing_logical_selection()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("long.txt", format!("{} old\n", "source prose ".repeat(60)))?;
    r.commit()?;
    r.write("long.txt", format!("{} new\n", "source prose ".repeat(60)))?;
    let hash = r.commit()?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    let key = DiffKey::Commit {
        hash,
        path: "long.txt".into(),
    };
    let data = crate::repo::diff::load_lines(
        &r.0,
        &key,
        Some(&[(1, 1)]),
        crate::git::DiffOptions::default(),
        true,
    )?;
    let mut view = crate::repo::DiffView::new(key);
    view.next_request();
    view.lines = Some(vec![(1, 1)]);
    let target = data
        .lines
        .iter()
        .position(|l| l.new == Some(1))
        .ok_or("logical line")?;
    view.chosen.insert(target);
    view.data = Some(Ok(data));
    tab.diff = Some(view);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 950.))
        .build_ui_state(
            |ui, tab: &mut RepoTab| {
                let fonts = egui::Id::new("line-history-test-fonts");
                if !ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(fonts))
                    .unwrap_or(false)
                {
                    crate::theme::install_fonts(ui.ctx());
                    ui.ctx().data_mut(|d| d.insert_temp(fonts, true));
                    return;
                }
                let mut acts = Vec::new();
                tab.diff_view(ui, &mut acts);
                for action in acts {
                    tab.apply(action, ui.ctx());
                }
            },
            tab,
        );
    h.run_steps(2);
    let before = crate::settings::Settings::get(&h.ctx).diff_wrap;
    h.get_by_label("Wrap").click();
    h.run_steps(2);
    assert_ne!(crate::settings::Settings::get(&h.ctx).diff_wrap, before);
    let view = h.state().diff.as_ref().ok_or("diff")?;
    assert_eq!(view.lines, Some(vec![(1, 1)]));
    assert_eq!(view.chosen, [target].into());
    assert!(view.data.as_ref().is_some_and(Result::is_ok));
    h.get_by_label("Wrap").click();
    h.run_steps(2);
    assert_eq!(crate::settings::Settings::get(&h.ctx).diff_wrap, before);
    Ok(())
}
