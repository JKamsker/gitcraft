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
