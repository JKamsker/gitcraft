use super::*;

/// The diff when it has lines to show; otherwise shows a spinner, the error, the LFS objects,
/// the images or a "no changes" note in its place.
pub(super) fn ready_data<'a>(
    ui: &mut Ui,
    view: &'a DiffView,
    ignore_ws: bool,
    t: &Tokens,
    find: &mut find::Find,
    acts: &mut Vec<Action>,
) -> Option<&'a DiffData> {
    let data = match &view.data {
        None => {
            ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
            return None;
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(
                    t.danger,
                    crate::git::display_error(&crate::forge::inbox::display_error(e)),
                );
            });
            return None;
        }
        Some(Ok(data)) => data,
    };
    if find.open {
        let identity = format!("rich-data:{:?}:{}", view.key, view.request);
        if let Some(binary) = &data.binary {
            find.rendered(ui, identity, |t| {
                super::super::binary::search_document(binary, t)
            });
            return None;
        }
        if let Some(notebook) = &data.notebook {
            find.rendered(ui, identity, |t| {
                super::super::notebook::search_document(notebook, t)
            });
            return None;
        }
        if let Some(lfs) = &data.lfs {
            find.rendered(ui, identity, |t| {
                let mut doc = find::rendered::Document::default();
                for (side, pointer) in [
                    (crate::i18n::text("repo-core-1564"), &lfs.old),
                    (crate::i18n::text("repo-core-1565"), &lfs.new),
                ] {
                    if let Some(pointer) = pointer {
                        doc.text(
                            &format!(
                                "{side} · Git LFS · {} · {}",
                                crate::git::human_size(pointer.size),
                                pointer.oid
                            ),
                            t.text,
                        );
                    }
                }
                doc
            });
            return None;
        }
        if data.compare.is_some() || data.image.is_some() {
            ui.label(crate::i18n::text("repo-core-2404"));
        }
    }
    if let Some(lfs) = &data.lfs {
        lfs_view(ui, lfs);
        return None;
    }
    if let Some(cmp) = &data.compare {
        compare_view(ui, cmp, acts);
        return None;
    }
    if let Some(images) = &data.image {
        if images.svg {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(!view.svg_source, crate::i18n::text("repo-core-2405"))
                    .clicked()
                {
                    acts.push(Action::DiffSvgSource(false));
                }
                if ui
                    .selectable_label(view.svg_source, crate::i18n::text("repo-core-2406"))
                    .clicked()
                {
                    acts.push(Action::DiffSvgSource(true));
                }
            });
        }
        if !images.svg || !view.svg_source {
            image_view(ui, images);
            return None;
        }
    }
    if let Some(binary) = &data.binary {
        binary_view(ui, binary, view.key.path());
        return None;
    }
    // A notebook shows its cells, unless its JSON is picked.
    if let Some(nb) = &data.notebook
        && !notebook_bar(ui, nb, view.key.path(), acts)
    {
        return None;
    }
    if data.lines.is_empty() {
        ui.centered_and_justified(|ui| {
            let msg = if ignore_ws {
                crate::i18n::text("repo-core-2407")
            } else {
                crate::i18n::text("repo-core-2408")
            };
            ui.label(egui::RichText::new(msg).color(t.text_dim));
        });
        return None;
    }
    Some(data)
}

/// The panel below the diff to write a comment on the line picked in a pull request's file.
pub(super) fn line_comment_panel(
    ui: &mut Ui,
    t: &Tokens,
    pr: &mut PrView,
    draft: &mut PrDraft,
    data: &DiffData,
    acts: &mut Vec<Action>,
) {
    egui::Panel::bottom("pr_line_comment")
        .frame(
            egui::Frame::new()
                .fill(t.card)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            // The line as it is in the pull request, to suggest a change to.
            let current = draft
                .target
                .as_ref()
                .filter(|t| t.side == DiffSide::Right)
                .and_then(|t| {
                    data.lines
                        .iter()
                        .find(|l| l.kind != Kind::Del && l.new.map(u64::from) == Some(t.line))
                        .and_then(|l| l.raw.get(1..))
                });
            pr_review::line_panel(ui, pr, draft, current, acts);
            pr_view::only_web_links(ui.ctx());
        });
}
