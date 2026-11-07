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
                "Reflog",
                "Where HEAD and each branch have been",
                Source::head(),
                ReflogAct::Show(Source::head()),
            ),
            (
                "search",
                "Find lost commits",
                "Commits no branch, tag or stash has any more",
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
                title: "RECOVERY",
                count: found,
                default_open: false,
                button: None,
            },
        );
        if !open {
            return;
        }
        if ui
            .button("Lost and found by content…")
            .on_hover_text(
                "Search remembered text in reflogs, dropped stashes and unreachable blobs",
            )
            .clicked()
        {
            acts.push(Action::OpenContentRecovery);
        }
        let t = Tokens::get(ui.ctx());
        let shown = self.reflog.as_ref().map(|v| &v.source);
        for (icon, title, about, source, act) in rows {
            let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 36.0));
            let r = ui.interact(rect, ui.id().with(("recovery-row", title)), Sense::click());
            let on = matches!(
                (&source, shown),
                (Source::Lost, Some(Source::Lost)) | (Source::Ref(_), Some(Source::Ref(_)))
            );
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
