//! Document responsibilities for continuous.
use super::*;

pub(super) fn working_keys(work: git::WorkStatus) -> Vec<DiffKey> {
    work.unstaged
        .into_iter()
        .chain(work.staged)
        .chain(work.conflicted)
        .map(DiffKey::Work)
        .collect()
}
pub(super) fn range_keys(
    from: &str,
    to: Option<&str>,
    files: Vec<git::ChangedFile>,
) -> Vec<DiffKey> {
    files
        .into_iter()
        .map(|f| DiffKey::Range {
            from: from.into(),
            to: to.map(str::to_owned),
            path: f.path,
        })
        .collect()
}
pub(super) fn check_budget(rows: usize, bytes: usize) -> Result<(), String> {
    if rows > 250_000 || bytes > 32 * 1024 * 1024 {
        Err("Review exceeds 250,000 lines or 32 MiB; select a narrower comparison. No partial document was accepted.".into())
    } else {
        Ok(())
    }
}
pub(super) fn visible_rows(
    count: usize,
    top: f32,
    clip_top: f32,
    clip_bottom: f32,
    height: f32,
) -> std::ops::Range<usize> {
    let start = ((clip_top - top).max(0.0) / height).floor() as usize;
    let end = ((clip_bottom - top).max(0.0) / height).ceil() as usize;
    start.min(count)..end.saturating_add(1).min(count).max(start.min(count))
}
pub(super) fn file_intro(
    ui: &mut Ui,
    key: &DiffKey,
    data: &diff::DiffData,
    acts: &mut Vec<Action>,
) -> egui::Response {
    let response = ui.button(crate::i18n::text(
        "repo-workflows-review-label-open-file-diff-5dcae6",
    ));
    if response.clicked() {
        acts.push(Action::OpenDiff(key.clone()));
    }
    metadata_ui(ui, key, data);
    response
}
pub(super) fn display_area(area: Area) -> &'static str {
    match area {
        Area::Staged => crate::i18n::text("repo-workflows-review-label-staged-daec45"),
        Area::Unstaged => crate::i18n::text("repo-workflows-review-label-unstaged-06b7ba"),
        Area::Untracked => crate::i18n::text("repo-workflows-review-label-untracked-c7ba54"),
        Area::Conflicted => crate::i18n::text("repo-workflows-review-label-conflict-014659"),
    }
}
pub(super) fn metadata_ui(ui: &mut Ui, key: &DiffKey, data: &diff::DiffData) {
    if let DiffKey::Work(file) = key {
        ui.label(crate::i18n::message(
            "repo-workflows-review-message-status-code-area-52cf53",
            &[
                ("code", (file.code.to_string()).into()),
                ("area", (display_area(file.area)).into()),
            ],
        ));
        if let Some(old) = &file.old_path {
            ui.label(format!("{old} → {}", file.path));
        }
    }
    if !data.header.is_empty() {
        ui.monospace(&data.header);
    }
    if data.lines.is_empty() {
        ui.weak(if data.binary.is_some() || data.image.is_some() || data.lfs.is_some() {
            crate::i18n::text("repo-workflows-review-label-binary-image-or-lfs-content-open-the-file-comp-3596ad")
        } else {
            crate::i18n::text("repo-workflows-review-label-no-textual-hunks-review-the-metadata-above-for-c3487e")
        });
    }
}
pub(super) fn picks(key: &DiffKey, partial: bool, revertable: bool) -> &'static [diff::Pick] {
    match key {
        DiffKey::Work(f) if partial && f.area == Area::Staged => &[diff::Pick::Unstage],
        DiffKey::Work(_) if partial => &[diff::Pick::Stage, diff::Pick::Discard],
        DiffKey::Commit { .. } if revertable => &[diff::Pick::Revert],
        _ => &[],
    }
}
