//! The Contained in section: the first release after the commit, and the branches and tags
//! that have it.

use egui::{RichText, Ui};

use super::ExtrasCmd;
use super::section::{self, Summary};
use crate::git::{Containment, RefList};
use crate::repo::widgets;
use crate::theme::Tokens;

pub(super) fn section(
    ui: &mut Ui,
    contained: Option<&Result<Containment, String>>,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    let summary = match contained {
        None => Summary::Loading,
        Some(Err(_)) => Summary::Text(crate::i18n::text("repo-core-1081").to_owned(), t.danger),
        Some(Ok(c)) => Summary::Text(headline(c), t.text_dim),
    };
    let mut again = false;
    section::section(
        ui,
        crate::i18n::text("repo-core-1082"),
        summary,
        false,
        Some((crate::i18n::text("repo-core-1083"), &mut again)),
        |ui| match contained {
            None => section::faint(ui, crate::i18n::text("repo-core-1084")),
            Some(Err(e)) => section::error(ui, e),
            Some(Ok(c)) => body(ui, c),
        },
    );
    if again {
        cmds.push(ExtrasCmd::ReloadContained);
    }
}

/// "2 branches · 3 tags · first tag v1.0".
pub(super) fn headline(c: &Containment) -> String {
    let branches = c.local.total + c.remote.total;
    let mut parts = vec![count(branches, "branch", "branches")];
    parts.push(count(c.tags.total, "tag", "tags"));
    if let Some(tag) = c.first_tag() {
        parts.push(crate::i18n::message("repo-core-1087", &[("tag", format!("{}", tag).into())]));
    }
    parts.join(" · ")
}

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => crate::i18n::message("repo-core-1088", &[("many", format!("{}", many).into())]),
        1 => crate::i18n::message("repo-core-1089", &[("one", format!("{}", one).into())]),
        n => crate::i18n::message("repo-core-1090", &[("n", (n).into()), ("many", format!("{}", many).into())]),
    }
}

fn body(ui: &mut Ui, c: &Containment) {
    let t = Tokens::get(ui.ctx());
    match (&c.describe, c.first_tag()) {
        (Some(describe), Some(tag)) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(crate::i18n::text("repo-core-1091")).color(t.text_faint));
                widgets::chip(ui, tag, t.success)
                    .on_hover_text(crate::i18n::message("repo-core-1092", &[("describe", format!("{}", describe).into())]));
            });
        }
        _ => section::faint(ui, crate::i18n::text("repo-core-1093")),
    }
    names(ui, crate::i18n::text("repo-core-1094"), &c.local, t.accent);
    names(ui, crate::i18n::text("repo-core-1095"), &c.remote, t.text_dim);
    names(ui, crate::i18n::text("repo-core-1096"), &c.tags, t.text_dim);
    if c.local.total + c.remote.total == 0 {
        if c.tags.total > 0 {
            section::faint(ui, crate::i18n::text("repo-core-1097"));
        } else {
            section::faint(
                ui,
                crate::i18n::text("repo-core-1098"),
            );
        }
    }
}

/// The names of `list` as chips under `title`, and how many more there are.
fn names(ui: &mut Ui, title: &str, list: &RefList, color: egui::Color32) {
    if list.total == 0 {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(title).color(t.text_faint));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for name in &list.names {
            widgets::chip(ui, name, color);
        }
        if list.hidden() > 0 {
            ui.label(
                RichText::new(crate::i18n::message("repo-core-1099", &[("hidden", format!("{}", list.hidden()).into())]))
                    .color(t.text_faint)
                    .small(),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headline_counts_branches_and_tags() {
        let mut c = Containment::default();
        assert_eq!(headline(&c), "no branches · no tags");
        c.local.total = 1;
        c.remote.total = 2;
        c.tags.total = 1;
        c.describe = Some("v1.0~2".to_owned());
        assert_eq!(headline(&c), "3 branches · 1 tag · first tag v1.0");
    }
}
