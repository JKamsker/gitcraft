//! The coverage map as tables: every changed file and hunk with what is known of its lines,
//! and the tests the report names. A hunk's row opens its diff, a file's the file at the
//! revision, a test's makes its hunks stand out.

use egui::Ui;

use super::picture::{self, NodeRef};
use super::{open_hunk, open_node};
use crate::insights::coverage::Coverage;
use crate::repo::Action;
use crate::repo::insights::common;

const ROW_H: f32 = 22.0;

pub(super) fn show(ui: &mut Ui, data: &Coverage, test: Option<usize>, acts: &mut Vec<Action>) {
    let tests: &[String] = data.report.as_ref().map_or(&[], |r| &r.tests);
    common::heading(ui, &format!("Changed files ({})", data.files.len()));
    let widths = [320.0, 70.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "File".into(),
            "Hunks".into(),
            "In report".into(),
            "Lines".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("coverage-files")
        .max_height(170.0)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_H, data.files.len(), |ui, rows| {
            for fi in rows {
                let f = &data.files[fi];
                let mut total = Default::default();
                for h in &f.hunks {
                    crate::insights::coverage::Counts::add(&mut total, h.counts());
                }
                let cells = [
                    f.path.clone(),
                    f.hunks.len().to_string(),
                    match f.in_report {
                        None => "no report",
                        Some(false) => "no",
                        Some(true) if f.by_suffix => "yes (by name end)",
                        Some(true) => "yes",
                    }
                    .to_owned(),
                    picture::counts_text(total),
                ];
                if common::table_row(ui, &widths, &cells, false, false)
                    .on_hover_text("Open the file at the revision")
                    .clicked()
                {
                    acts.extend(open_node(data, NodeRef::File(fi), test));
                }
            }
        });

    let hunks: Vec<(usize, usize)> = data
        .files
        .iter()
        .enumerate()
        .flat_map(|(fi, f)| (0..f.hunks.len()).map(move |hi| (fi, hi)))
        .filter(|&(fi, hi)| test.is_none_or(|t| data.files[fi].hunks[hi].tests.contains(&t)))
        .collect();
    common::heading(ui, &format!("Hunks ({})", hunks.len()));
    let widths = [260.0, 90.0, 70.0, 80.0, 110.0, 70.0];
    common::table_row(
        ui,
        &widths,
        &[
            "File".into(),
            "Lines".into(),
            "Covered".into(),
            "Uncovered".into(),
            "Not instrumented".into(),
            "Unknown".into(),
            "Tests".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("coverage-hunks")
        .max_height(260.0)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_H, hunks.len(), |ui, rows| {
            for &(fi, hi) in &hunks[rows] {
                let f = &data.files[fi];
                let h = &f.hunks[hi];
                let c = h.counts();
                let names: Vec<&str> = h
                    .tests
                    .iter()
                    .filter_map(|&i| tests.get(i).map(String::as_str))
                    .collect();
                let cells = [
                    f.path.clone(),
                    picture::hunk_label(h),
                    c.covered.to_string(),
                    c.uncovered.to_string(),
                    c.not_instrumented.to_string(),
                    c.unknown.to_string(),
                    names.join(", "),
                ];
                if common::table_row(ui, &widths, &cells, false, false)
                    .on_hover_text("Open its diff")
                    .clicked()
                {
                    acts.extend(open_hunk(data, fi, hi));
                }
            }
        });

    if data.report.as_ref().is_some_and(|r| r.per_test) {
        common::heading(ui, &format!("Tests the report names ({})", tests.len()));
        for (ti, name) in tests.iter().enumerate() {
            let ran = hunks_of(data, ti);
            let cells = [name.clone(), format!("{ran} changed hunks")];
            if common::table_row(ui, &[320.0], &cells, false, test == Some(ti))
                .on_hover_text("Show only the hunks it ran")
                .clicked()
            {
                acts.extend(open_node(data, NodeRef::Test(ti), test));
            }
        }
    }
}

/// How many changed hunks test `t` ran.
fn hunks_of(data: &Coverage, t: usize) -> usize {
    data.files
        .iter()
        .flat_map(|f| &f.hunks)
        .filter(|h| h.tests.contains(&t))
        .count()
}
