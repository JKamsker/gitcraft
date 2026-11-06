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
        Some(Err(_)) => Summary::Text("Could not be read".to_owned(), t.danger),
        Some(Ok(c)) => Summary::Text(headline(c), t.text_dim),
    };
    let mut again = false;
    section::section(
        ui,
        "Contained in",
        summary,
        false,
        Some(("Read again", &mut again)),
        |ui| match contained {
            None => section::faint(ui, "Looking for the branches and tags that have it…"),
            Some(Err(e)) => section::error(ui, e),
            Some(Ok(c)) => body(ui, c),
        },
    );
    if again {
        cmds.push(ExtrasCmd::ReloadContained);
    }
}

/// "2 branches · 3 tags · first in v1.0".
pub(super) fn headline(c: &Containment) -> String {
    let branches = c.local.total + c.remote.total;
    let mut parts = vec![count(branches, "branch", "branches")];
    parts.push(count(c.tags.total, "tag", "tags"));
    if let Some(tag) = c.first_tag() {
        parts.push(format!("first in {tag}"));
    }
    parts.join(" · ")
}

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

fn body(ui: &mut Ui, c: &Containment) {
    let t = Tokens::get(ui.ctx());
    match (&c.describe, c.first_tag()) {
        (Some(describe), Some(tag)) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("First release").color(t.text_faint));
                widgets::chip(ui, tag, t.success)
                    .on_hover_text(format!("git describe --contains: {describe}"));
            });
        }
        _ => section::faint(ui, "No tag contains it yet: it isn't in a release."),
    }
    names(ui, "Branches", &c.local, t.accent);
    names(ui, "Remote branches", &c.remote, t.text_dim);
    names(ui, "Tags", &c.tags, t.warning);
    if c.local.total + c.remote.total == 0 {
        section::faint(ui, "No branch has it: only tags, or nothing, keep it.");
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
                RichText::new(format!("and {} more", list.hidden()))
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
        assert_eq!(headline(&c), "3 branches · 1 tag · first in v1.0");
    }
}
