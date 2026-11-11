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

include!("tests_widgets.rs");
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

include!("tests_boundaries.rs");
include!("tests_navigation.rs");
