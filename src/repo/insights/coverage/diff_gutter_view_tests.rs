//! The real diff view, headless: a fixture repository's coverage report read through the
//! gutter's background load and painted as marks beside the changed lines, or, for a report
//! of another commit, as unknown under the stale chip.

use egui_kittest::{Harness, kittest::Queryable};

use crate::repo::diff::coverage::{CoverageAct, Mark};
use crate::repo::diff::{DiffData, DiffLine, Kind};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, messages};
use crate::settings::Settings;

fn line(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    let mark = match kind {
        Kind::Add => '+',
        Kind::Del => '-',
        _ => ' ',
    };
    DiffLine {
        kind,
        old,
        new,
        text: text.into(),
        raw: format!("{mark}{text}"),
    }
}

/// The diff of `src/lib.rs` in `hash`: lines 1–3 added.
fn data() -> DiffData {
    let lines = vec![
        line(Kind::Hunk, None, None, "@@ -0,0 +1,3 @@"),
        line(Kind::Add, None, Some(1), "fn a() {"),
        line(Kind::Add, None, Some(2), "    1"),
        line(Kind::Add, None, Some(3), "}"),
    ];
    let mut data = DiffData::default();
    data.spans = vec![None; lines.len()];
    data.changed = vec![None; lines.len()];
    data.split = crate::repo::diff::comparison_pairs(&lines);
    data.lines = lines;
    data
}

/// The diff view of `hash` with the marks on and `split` as set; each frame's coverage load
/// runs at once, its job included, so the next frame paints what it read.
fn harness(repo: &std::path::Path, hash: &str, split: bool) -> Harness<'static, RepoTab> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.to_path_buf(), &ctx);
    let mut view = DiffView::new(DiffKey::Commit {
        hash: hash.to_owned(),
        path: "src/lib.rs".into(),
    });
    view.data = Some(Ok(data()));
    tab.diff = Some(view);
    let mut styled = false;
    Harness::builder().with_size([900.0, 400.0]).build_ui_state(
        move |ui, tab: &mut RepoTab| {
            if !styled {
                crate::theme::install_fonts(ui.ctx());
                let mut settings = Settings::get(ui.ctx());
                settings.diff_coverage = true;
                settings.diff_split = split;
                settings.put_for_test(ui.ctx());
                styled = true;
                return;
            }
            let mut acts = Vec::new();
            egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut acts));
            let (_, jobs) = messages::record_jobs(|| {
                for act in acts {
                    if matches!(act, Action::DiffCoverage(CoverageAct::Load)) {
                        tab.apply(act, ui.ctx());
                    }
                }
            });
            for job in jobs {
                tab.handle(job(), ui.ctx());
            }
            crate::a11y::end_frame(ui.ctx());
        },
        tab,
    )
}

/// How many marks of `mark` the diff shows, by their accessible names.
fn count(h: &Harness<'_, RepoTab>, mark: Mark) -> usize {
    let name = match mark {
        Mark::Covered(hits) => crate::i18n::message(
            "diff-coverage-mark-covered",
            &[("hits", crate::i18n::Value::from(hits))],
        ),
        Mark::Uncovered => crate::i18n::text("diff-coverage-mark-uncovered").to_owned(),
        Mark::Unknown => crate::i18n::text("diff-coverage-mark-unknown").to_owned(),
    };
    h.query_all_by_label(&name).count()
}

#[test]
fn the_diff_gutter_marks_changed_lines_from_a_fixture_report() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage-gutter")?;
    repo.commit("one", &[(".gitignore", Some("lcov.info*\n"))])?;
    let two = repo.commit("two", &[("src/lib.rs", Some("fn a() {\n    1\n}\n"))])?;
    let write = |name: &str, text: &str| {
        std::fs::write(repo.path().join(name), text).map_err(|e| e.to_string())
    };
    write(
        "lcov.info",
        "SF:src/lib.rs\nDA:1,3\nDA:2,0\nend_of_record\n",
    )?;
    write("lcov.info.sha", &two)?;
    for split in [false, true] {
        let mut h = harness(repo.path(), &two, split);
        h.run_steps(4);
        assert_eq!(count(&h, Mark::Covered(3)), 1, "split: {split}");
        assert_eq!(count(&h, Mark::Uncovered), 1, "split: {split}");
        // Line 3 isn't in the report: not executable, so not marked at all.
        assert_eq!(count(&h, Mark::Unknown), 0, "split: {split}");
        let chip = crate::i18n::message(
            "diff-coverage-current",
            &[
                ("report", "lcov.info".into()),
                ("sha", two.chars().take(8).collect::<String>().into()),
            ],
        );
        assert!(h.query_by_label(&chip).is_some(), "split: {split}");
    }

    // A commit after the one tested: the report is stale, so it marks nothing as covered.
    let three = repo.commit("three", &[("src/lib.rs", Some("fn a() {\n    2\n}\n"))])?;
    let mut h = harness(repo.path(), &three, false);
    h.run_steps(4);
    assert_eq!(count(&h, Mark::Covered(3)), 0);
    assert_eq!(count(&h, Mark::Uncovered), 0);
    // Every changed line, executable or not: the report can't say which is, for this commit.
    assert_eq!(count(&h, Mark::Unknown), 3);
    let stale = crate::i18n::message(
        "diff-coverage-stale-other",
        &[
            ("report", "lcov.info".into()),
            ("sha", two.chars().take(8).collect::<String>().into()),
        ],
    );
    assert!(h.query_by_label(&stale).is_some(), "{stale}");

    // Off: no marks and no chip, whatever the report says.
    let mut h = harness(repo.path(), &two, false);
    h.run_steps(4);
    let ctx = h.ctx.clone();
    h.state_mut()
        .apply(Action::DiffCoverage(CoverageAct::Toggle), &ctx);
    h.run_steps(2);
    assert!(!Settings::get(&h.ctx).diff_coverage);
    assert_eq!(count(&h, Mark::Covered(3)), 0);
    assert!(h.query_by_label(&stale).is_none());
    Ok(())
}
