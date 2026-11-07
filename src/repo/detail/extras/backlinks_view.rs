//! The Backlinks section: the later commits that mention this one, the explicit links
//! (reverts, cherry-picks, trailers) apart from bare mentions. A click opens one.

use egui::{RichText, Sense, Ui};

use super::section::{self, Summary};
use super::{Backlinks, ExtrasCmd};
use crate::git::{BACKLINK_CAP, Backlink};
use crate::repo::short;
use crate::theme::{self, Tokens};

pub(super) fn section(
    ui: &mut Ui,
    backlinks: Option<&Result<Backlinks, String>>,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    let summary = match backlinks {
        None => Summary::Loading,
        Some(Err(_)) => Summary::Text("Could not be searched".to_owned(), t.danger),
        Some(Ok((links, more))) => {
            let color = if links.is_empty() {
                t.text_faint
            } else {
                t.text_dim
            };
            Summary::Text(headline(links, *more), color)
        }
    };
    let mut again = false;
    section::section(
        ui,
        "Backlinks",
        summary,
        false,
        Some(("Search again", &mut again)),
        |ui| match backlinks {
            None => section::faint(ui, "Searching later commits for its hash…"),
            Some(Err(e)) => section::error(ui, e),
            Some(Ok((links, more))) => body(ui, links, *more, cmds),
        },
    );
    if again {
        cmds.push(ExtrasCmd::ReloadBacklinks);
    }
}

/// "1 link · 2 mentions".
pub(super) fn headline(links: &[Backlink], more: bool) -> String {
    if links.is_empty() {
        return "no later commit mentions it".to_owned();
    }
    let explicit = links.iter().filter(|l| l.kind.explicit()).count();
    let bare = links.len() - explicit;
    let mut parts = Vec::new();
    if explicit > 0 {
        parts.push(format!(
            "{explicit} link{}",
            if explicit == 1 { "" } else { "s" }
        ));
    }
    if bare > 0 {
        parts.push(format!(
            "{bare} mention{}",
            if bare == 1 { "" } else { "s" }
        ));
    }
    let text = parts.join(" · ");
    if more {
        format!("{text} · only the {BACKLINK_CAP} most recent")
    } else {
        text
    }
}

fn body(ui: &mut Ui, links: &[Backlink], more: bool, cmds: &mut Vec<ExtrasCmd>) {
    if links.is_empty() {
        section::faint(
            ui,
            "No later commit mentions its hash: no revert, cherry-pick, Fixes: trailer or plain mention.",
        );
        return;
    }
    let (explicit, bare): (Vec<&Backlink>, Vec<&Backlink>) =
        links.iter().partition(|l| l.kind.explicit());
    group(ui, "Links", &explicit, cmds);
    group(ui, "Mentions", &bare, cmds);
    if more {
        section::faint(
            ui,
            format!("Only the {BACKLINK_CAP} most recent are listed."),
        );
    }
}

fn group(ui: &mut Ui, title: &str, links: &[&Backlink], cmds: &mut Vec<ExtrasCmd>) {
    if links.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(title).color(t.text_faint));
    for link in links {
        if row(ui, link) {
            cmds.push(ExtrasCmd::Open(link.hash.clone()));
        }
    }
}

/// One linking commit: how it links, its hash and subject, the line that mentions the
/// commit. Returns whether it was clicked.
fn row(ui: &mut Ui, link: &Backlink) -> bool {
    let t = Tokens::get(ui.ctx());
    // Behind the row: painted once it is known to be hovered.
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(4, 3))
        .corner_radius(t.radius_sm)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    RichText::new(short(&link.hash))
                        .font(theme::mono(11.5))
                        .color(t.accent),
                );
                ui.label(RichText::new(link.kind.label()).color(t.text_faint).small());
                ui.add(egui::Label::new(&link.subject).truncate());
            });
            // The line with the hash, unless it is the subject shown already.
            if link.text != link.subject.trim() {
                ui.add(
                    egui::Label::new(
                        RichText::new(&link.text)
                            .font(theme::mono(11.0))
                            .color(t.text_faint),
                    )
                    .truncate(),
                );
            }
        });
    let resp = ui
        .interact(
            inner.response.rect,
            ui.id().with(("backlink", &link.hash)),
            Sense::click(),
        )
        .on_hover_text(format!(
            "{} · {} · {}\n\n{}\n\nClick to open it",
            short(&link.hash),
            link.author,
            link.date,
            link.text
        ));
    if resp.hovered() {
        ui.painter().set(
            background,
            egui::Shape::rect_filled(inner.response.rect, t.radius_sm, t.hover),
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::LinkKind;

    fn link(kind: LinkKind) -> Backlink {
        Backlink {
            hash: "a".repeat(40),
            subject: "s".to_owned(),
            author: "a".to_owned(),
            date: "d".to_owned(),
            kind,
            text: "t".to_owned(),
        }
    }

    #[test]
    fn the_headline_counts_links_apart_from_mentions() {
        assert_eq!(headline(&[], false), "no later commit mentions it");
        let links = [
            link(LinkKind::Revert),
            link(LinkKind::Trailer("Fixes".into())),
            link(LinkKind::Mention),
        ];
        assert_eq!(headline(&links, false), "2 links · 1 mention");
        assert_eq!(
            headline(&links[2..], true),
            "1 mention · only the 200 most recent"
        );
    }
}
