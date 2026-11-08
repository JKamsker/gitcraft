//! The impact graph as tables: the changed symbols, every reference kept (also those the
//! picture had no room for), and the files no analyzer reads. Each row opens what it is at the
//! revision it was read at.

use egui::Ui;

use super::picture::Filter;
use super::{file_diff, open_node, open_ref};
use crate::insights::impact::Impact;
use crate::repo::insights::common;
use crate::repo::{Action, short};

const ROW_H: f32 = 22.0;

pub(super) fn show(ui: &mut Ui, data: &Impact, filter: &Filter, acts: &mut Vec<Action>) {
    let symbols = filter.symbols(data);
    common::heading(ui, &format!("Changed symbols ({})", symbols.len()));
    let widths = [80.0, 90.0, 260.0, 60.0, 80.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Change".into(),
            "Kind".into(),
            "Symbol".into(),
            "Public".into(),
            "References".into(),
            "Defined at".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("impact-symbols")
        .max_height(180.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for &i in &symbols {
                let s = &data.symbols[i];
                let refs = data.refs.iter().filter(|r| r.symbol == i).count();
                let at = s.at().map_or(String::new(), |sp| {
                    format!("{}:{} @ {}", sp.path, sp.line, short(&sp.rev))
                });
                let cells = [
                    s.change.word().to_owned(),
                    s.kind.word().to_owned(),
                    s.path.clone(),
                    if s.public { "pub" } else { "" }.to_owned(),
                    format!("{refs}{}", if s.refs_cut { "+" } else { "" }),
                    at,
                ];
                if common::table_row(ui, &widths, &cells, false, false)
                    .on_hover_text("Open its definition at that revision")
                    .clicked()
                {
                    acts.extend(open_node(data, super::NodeRef::Symbol(i)));
                }
            }
        });

    let refs = filter.refs(data);
    common::heading(
        ui,
        &format!("References by name ({}, inferred)", refs.len()),
    );
    let widths = [160.0, 200.0, 60.0, 90.0, 240.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Symbol".into(),
            "From".into(),
            "Kind".into(),
            "Its file".into(),
            "Line".into(),
            "Text".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("impact-refs")
        .max_height(260.0)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_H, refs.len(), |ui, rows| {
            for &r in &refs[rows] {
                let reference = &data.refs[r];
                let site = &data.sites[reference.site];
                let cells = [
                    data.symbols[reference.symbol].name.clone(),
                    site.label.clone(),
                    if site.test { "test" } else { "code" }.to_owned(),
                    if site.in_diff { "changed" } else { "unchanged" }.to_owned(),
                    format!(
                        "{}:{} @ {}",
                        reference.path,
                        reference.line,
                        short(&reference.rev)
                    ),
                    reference.text.clone(),
                ];
                if common::table_row(ui, &widths, &cells, false, false)
                    .on_hover_text("Open this line at that revision")
                    .clicked()
                {
                    acts.push(open_ref(reference));
                }
            }
        });

    if !data.other.is_empty() {
        common::heading(
            ui,
            &format!("Files no analyzer reads ({})", data.other.len()),
        );
        let widths = [60.0, 320.0];
        common::table_row(
            ui,
            &widths,
            &["Status".into(), "File".into(), "Why unknown".into()],
            true,
            false,
        );
        for other in &data.other {
            let cells = [
                other.status.to_string(),
                other.path.clone(),
                other.why.clone(),
            ];
            if common::table_row(ui, &widths, &cells, false, false)
                .on_hover_text("Open its diff")
                .clicked()
            {
                acts.push(file_diff(data, &other.path));
            }
        }
    }
}
