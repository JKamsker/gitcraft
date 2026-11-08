//! The Backlinks section: the later commits that mention this one, the explicit links
//! (reverts, cherry-picks, trailers) apart from bare mentions. A click opens one.

use egui::{RichText, Sense, Ui};

use super::section::{self, Summary};
use super::{Backlinks, ExtrasCmd};
use crate::a11y::Named;
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
        Some(Err(_)) => Summary::Text(crate::i18n::text("repo-core-1066").to_owned(), t.danger),
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
        crate::i18n::text("repo-core-1067"),
        summary,
        false,
        Some((crate::i18n::text("repo-core-1068"), &mut again)),
        |ui| match backlinks {
            None => section::faint(ui, crate::i18n::text("repo-core-1069")),
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
        return crate::i18n::text("repo-core-1070").to_owned();
    }
    let explicit = links.iter().filter(|l| l.kind.explicit()).count();
    let bare = links.len() - explicit;
    let mut parts = Vec::new();
    if explicit > 0 {
        parts.push(crate::i18n::message(
            "repo-core-1071",
            &[("explicit", (explicit).into())],
        ));
    }
    if bare > 0 {
        parts.push(crate::i18n::message(
            "repo-core-1072",
            &[("bare", (bare).into())],
        ));
    }
    let text = parts.join(" · ");
    if more {
        crate::i18n::message(
            "repo-core-1073",
            &[
                ("text", format!("{}", text).into()),
                ("BACKLINK-CAP", format!("{}", BACKLINK_CAP).into()),
            ],
        )
    } else {
        text
    }
}

fn body(ui: &mut Ui, links: &[Backlink], more: bool, cmds: &mut Vec<ExtrasCmd>) {
    if links.is_empty() {
        section::faint(ui, crate::i18n::text("repo-core-1074"));
        return;
    }
    let (explicit, bare): (Vec<&Backlink>, Vec<&Backlink>) =
        links.iter().partition(|l| l.kind.explicit());
    group(ui, crate::i18n::text("repo-core-1075"), &explicit, cmds);
    group(ui, crate::i18n::text("repo-core-1076"), &bare, cmds);
    if more {
        section::faint(
            ui,
            crate::i18n::message(
                "repo-core-1077",
                &[("BACKLINK-CAP", format!("{}", BACKLINK_CAP).into())],
            ),
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
        .named(|| {
            crate::i18n::message(
                "repo-core-1079",
                &[
                    ("hash", format!("{}", short(&link.hash)).into()),
                    ("subject", format!("{}", link.subject).into()),
                ],
            )
        })
        .on_hover_text(crate::i18n::message(
            "repo-core-1080",
            &[
                ("hash", format!("{}", short(&link.hash)).into()),
                ("author", format!("{}", link.author).into()),
                ("date", crate::i18n::date_iso(&link.date).into()),
                ("text", format!("{}", link.text).into()),
            ],
        ));
    // The root's action label and the interaction metadata describe this same button.
    // Keep one name; a second widget_info call would overwrite its existing action label.
    ui.ctx().accesskit_node_builder(resp.id, |node| {
        node.set_description(crate::a11y::labels::join([
            link.author.as_str(),
            &crate::i18n::date_iso(&link.date),
            link.text.as_str(),
        ]));
    });
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
    fn linking_commit_has_a_name_and_keyboard_action() {
        use egui_kittest::{Harness, kittest::Queryable};
        let link = link(LinkKind::Revert);
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, opened: &mut bool| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                *opened |= row(ui, &link);
                crate::a11y::end_frame(ui.ctx());
            },
            false,
        );
        h.run();
        let label = "Open changes in aaaaaaaa: s";
        h.get_by_role_and_label(egui::accesskit::Role::Button, label);
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label(label).is_focused());
        h.key_press(egui::Key::Space);
        h.run();
        assert!(*h.state());
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

#[cfg(test)]
mod headless_tests {
    use super::*;
    use crate::git::LinkKind;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn actual_backlink_row_click_emits_the_target_open_command_with_grouped_evidence() {
        let hash = "a".repeat(40);
        let links = vec![
            Backlink {
                hash: hash.clone(),
                subject: "Fix the target".into(),
                author: "Test".into(),
                date: "today".into(),
                kind: LinkKind::Trailer("Fixes".into()),
                text: "Fixes: 1234567".into(),
            },
            Backlink {
                hash: "b".repeat(40),
                subject: "Mention the target".into(),
                author: "Test".into(),
                date: "today".into(),
                kind: LinkKind::Mention,
                text: "Related to 1234567".into(),
            },
        ];
        let mut harness = Harness::builder().with_size([900.0, 700.0]).build_ui_state(
            move |ui, cmds: &mut Vec<ExtrasCmd>| body(ui, &links, false, cmds),
            Vec::new(),
        );
        harness.run();
        harness.get_by_label("Links");
        harness.get_by_label("Mentions");
        harness.get_by_label("Fixes: 1234567");
        harness
            .get_by_label("Open changes in aaaaaaaa: Fix the target")
            .click();
        harness.run();
        assert!(
            harness
                .state()
                .iter()
                .any(|cmd| matches!(cmd,ExtrasCmd::Open(target) if target==&hash))
        );
    }
}
