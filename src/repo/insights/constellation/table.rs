//! Constellation table, including on-demand resource totals.
use super::picture::{disk_line, flag_words};
use super::{Act, act};
use crate::insights::constellation::Model;
use crate::insights::constellation::read::Disk;
use crate::repo::{Action, insights::common};
use egui::Ui;
use std::collections::HashMap;

/// The table: a row per node, clickable.
pub(super) fn nodes_table(
    ui: &mut Ui,
    m: &Model,
    disk: Option<&HashMap<String, Result<Disk, String>>>,
    remote: bool,
    selected: Option<usize>,
    other: Option<usize>,
    acts: &mut Vec<Action>,
) {
    let widths = [100.0, 200.0, 160.0, 180.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Kind".into(),
            "Name".into(),
            "States".into(),
            crate::i18n::text("resources-logical").into(),
            "Joined to".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("constellation-table")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, n) in m.nodes.iter().enumerate() {
                let links: Vec<String> = m
                    .links_of(i)
                    .iter()
                    .map(|(e, j)| format!("{} {}", e.link.label(), m.nodes[*j].label))
                    .collect();
                let resp = common::table_row(
                    ui,
                    &widths,
                    &[
                        n.kind.label().to_owned(),
                        n.label.clone(),
                        flag_words(n),
                        disk_line(n, disk, remote).unwrap_or_default(),
                        links.join("; "),
                    ],
                    false,
                    selected == Some(i) || other == Some(i),
                );
                if resp.clicked() {
                    let add = ui.input(|inp| inp.modifiers.command);
                    acts.push(act(Act::Select { node: i, add }));
                }
            }
        });
}
