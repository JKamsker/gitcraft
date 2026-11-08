//! The way to the reflog view: HEAD's reflog, and the scan for lost commits.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use crate::icons;
use crate::repo::pinned_sections::{Header, Sections};
use crate::repo::reflog::{ReflogAct, Source};
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

impl RepoTab {
    pub(super) fn recovery_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        sections: &mut Sections,
    ) {
        let rows = [
            (
                "history",
                crate::i18n::text("repo-core-0058"),
                crate::i18n::text("repo-core-3470"),
                Source::head(),
                ReflogAct::Show(Source::head()),
            ),
            (
                "search",
                crate::i18n::text("repo-core-0060"),
                crate::i18n::text("repo-core-3471"),
                Source::Lost,
                ReflogAct::FindLost,
            ),
        ];
        // What the last scan found, still lost.
        let found = self
            .reflog
            .as_ref()
            .and_then(|v| v.lost_commits())
            .map_or(0, <[_]>::len);
        let open = sections.add(
            ui,
            Header {
                key: "recovery",
                icon: "history",
                title: crate::i18n::text("repo-core-3472"),
                count: found,
                default_open: false,
                button: None,
            },
        );
        if !open {
            return;
        }
        if ui
            .button(crate::i18n::text("repo-core-3473"))
            .on_hover_text(crate::i18n::text("repo-core-3474"))
            .clicked()
        {
            acts.push(Action::OpenContentRecovery);
        }
        if ui.button(crate::i18n::text("repo-core-3475")).clicked() {
            acts.push(Action::EarlierVersions);
        }
        let t = Tokens::get(ui.ctx());
        if ui.button(crate::i18n::text("repo-core-3476")).clicked() {
            acts.push(Action::OpenSymbolBiography(None));
        }
        if ui.button(crate::i18n::text("repo-core-3477")).clicked() {
            acts.push(Action::OpenLineDossier(None));
        }
        let shown = self.reflog.as_ref().map(|v| &v.source);
        for (icon, title, about, source, act) in rows {
            let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 36.0));
            let r = ui.interact(rect, ui.id().with(("recovery-row", icon)), Sense::click());
            let on = matches!(
                (&source, shown),
                (Source::Lost, Some(Source::Lost)) | (Source::Ref(_), Some(Source::Ref(_)))
            );
            crate::a11y::a11y_selected(&r, || format!("{title}: {about}"), on);
            if on {
                ui.painter().rect_filled(rect, 0.0, t.row_selected);
            } else if r.hovered() {
                ui.painter().rect_filled(rect, 0.0, t.hover);
            }
            let y = rect.center().y;
            icons::paint(
                ui,
                Rect::from_center_size(pos2(rect.left() + 34.0, y), vec2(16.0, 16.0)),
                icon,
                13.0,
                t.text_faint,
            );
            let p = ui.painter_at(rect);
            p.text(
                pos2(rect.left() + 48.0, y - 7.0),
                Align2::LEFT_CENTER,
                title,
                egui::FontId::proportional(12.5),
                t.text,
            );
            p.text(
                pos2(rect.left() + 48.0, y + 8.0),
                Align2::LEFT_CENTER,
                about,
                egui::FontId::proportional(11.0),
                t.text_faint,
            );
            let r = r.on_hover_text(about);
            if r.clicked() {
                acts.push(Action::Reflog(act));
            }
        }
    }
}
