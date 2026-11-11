//! Churn controls, provenance summary, and selected sample details.

use super::*;

/// The branch, the history limit, the samples, the rows and the filter.
pub(super) fn controls(
    ui: &mut Ui,
    state: &mut State,
    branches: &[String],
    acts: &mut Vec<Action>,
) {
    let push =
        |acts: &mut Vec<Action>, act: Act| acts.push(Action::Insights(InsightsAct::Churn(act)));
    ui.horizontal_wrapped(|ui| {
        let shown = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        egui::ComboBox::from_id_salt("insights-churn-rev")
            .selected_text(shown)
            .width(140.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.rev.is_none(), "HEAD (checked out)")
                    .clicked()
                    && state.rev.is_some()
                {
                    push(acts, Act::Rev(None));
                }
                for name in branches {
                    let on = state.rev.as_deref() == Some(name.as_str());
                    if ui.selectable_label(on, name).clicked() && !on {
                        push(acts, Act::Rev(Some(name.clone())));
                    }
                }
            })
            .response
            .on_hover_text("The branch whose history is read");
        let limit = state.limit();
        egui::ComboBox::from_id_salt("insights-churn-limit")
            .selected_text(format!("last {limit} commits"))
            .show_ui(ui, |ui| {
                for n in LIMITS {
                    if ui
                        .selectable_label(n == limit, format!("last {n} commits"))
                        .clicked()
                        && n != limit
                    {
                        push(acts, Act::Limit(n));
                    }
                }
            });
        let mut changed = false;
        let samples = state.options.samples;
        egui::ComboBox::from_id_salt("insights-churn-samples")
            .selected_text(format!("{samples} samples"))
            .show_ui(ui, |ui| {
                for n in core::SAMPLES {
                    if ui
                        .selectable_label(n == samples, format!("{n} samples"))
                        .clicked()
                    {
                        changed |= n != samples;
                        state.options.samples = n;
                    }
                }
            })
            .response
            .on_hover_text("Commits spread evenly over the history read, the tip last");
        ui.separator();
        let group = |g: Option<usize>| match g {
            None => "Files".to_owned(),
            Some(d) => format!("Folders, {d} deep"),
        };
        egui::ComboBox::from_id_salt("insights-churn-group")
            .selected_text(group(state.options.group))
            .show_ui(ui, |ui| {
                for g in std::iter::once(None).chain((1..=MAX_DEPTH).map(Some)) {
                    if ui
                        .selectable_label(state.options.group == g, group(g))
                        .clicked()
                    {
                        changed |= state.options.group != g;
                        state.options.group = g;
                    }
                }
            })
            .response
            .on_hover_text("Rows: files, or the folders (modules) they are in");
        let filter = ui.add(
            egui::TextEdit::singleline(&mut state.options.filter)
                .hint_text("Filter paths")
                .desired_width(150.0),
        );
        // Each change reads the samples again: only once typing stops (Enter or leaving).
        state.filter_edited |= filter.changed();
        if filter.lost_focus() && state.filter_edited {
            state.filter_edited = false;
            changed = true;
        }
        let rows = state.options.max_rows;
        egui::ComboBox::from_id_salt("insights-churn-rows")
            .selected_text(format!("{rows} rows at most"))
            .show_ui(ui, |ui| {
                for n in ROW_LIMITS {
                    if ui
                        .selectable_label(n == rows, format!("{n} rows at most"))
                        .clicked()
                    {
                        changed |= n != rows;
                        state.options.max_rows = n;
                    }
                }
            });
        if state.read.loading || state.built.loading {
            ui.add(crate::spinner::Spinner::new());
        }
        if changed {
            push(acts, Act::Rebuild);
        }
    });
}

/// What each measure is and how it is known, the hotspot rule, and the legend.
pub(super) fn explain(ui: &mut Ui, built: &Built) {
    let t = Tokens::get(ui.ctx());
    let faint = |s: String| RichText::new(s).size(11.5).color(t.text_dim);
    ui.add(
        egui::Label::new(faint(format!(
            "Size: lines at the sample (factual) · Churn: lines changed in the window of commits \
             up to it (factual) · Complexity: branch points, {} — Rust, C-like, Go and \
             JavaScript/TypeScript files only, others not measured · Tests: the test lane's verdict on the sample's tree, when \
             one was recorded",
            analyzer::LABEL
        )))
        .wrap(),
    );
    ui.add(
        egui::Label::new(faint(format!(
            "{}. Files over {} KiB aren't read, nor more than {} files or {} MiB in all: what \
             that leaves out is said above, and its points are lower bounds.",
            core::HOTSPOT_RULE,
            core::MAX_BLOB >> 10,
            core::MAX_BLOBS,
            core::MAX_BYTES >> 20
        )))
        .wrap(),
    );
    if let Some(e) = &built.timeline.tests_error {
        ui.label(
            RichText::new(format!(
                "Test results couldn't be read ({e}): shown as unknown"
            ))
            .size(11.5)
            .color(t.warning),
        );
    }
    common::legend(
        ui,
        &[
            (Mark::Glyph("!", t.warning), "hotspot"),
            (Mark::Fill(t.accent), "read in full"),
            (Mark::Hollow(t.accent), "lower bound: not all files read"),
            (Mark::Line(t.text_dim, true), "not there at that sample"),
            (Mark::Glyph("+", t.success), "tests passed"),
            (Mark::Glyph("x", t.danger), "tests failed"),
            (Mark::Glyph("?", t.text_faint), "no test result (unknown)"),
        ],
    );
}

/// The calendar day of `time`, UTC.
pub(super) fn day(time: i64) -> String {
    bucket_label(bucket(time, Axis::Day), Axis::Day)
}

/// The commits of window `k` of row `r` that changed it: each selects its commit, each of its
/// paths opens that file's diff. `false` when it was closed (or is gone).
pub(super) fn window(
    ui: &mut Ui,
    built: &Built,
    r: usize,
    k: usize,
    acts: &mut Vec<Action>,
) -> bool {
    let tl = &built.timeline;
    let (Some(row), Some(sample)) = (tl.rows.get(r), tl.samples.get(k)) else {
        return false;
    };
    let commits = row.commits_in(k);
    let window_len = tl.windows.get(k).map_or(0, Vec::len);
    let mut open = true;
    ui.horizontal_wrapped(|ui| {
        common::heading(
            ui,
            &format!(
                "{} · window {} of {} up to {} ({}): {} of its {} changed it",
                row.shown(),
                k + 1,
                tl.samples.len(),
                short(&sample.sample.hash),
                day(sample.sample.time),
                plural(commits.len(), "commit"),
                window_len,
            ),
        );
        if !row.folder
            && row.points.get(k).is_some_and(|p| p.size.is_some())
            && let Some(path) = row.paths.get(k).cloned()
            && ui
                .small_button("File at this sample")
                .on_hover_text("Open the file as it was at the sample commit")
                .clicked()
        {
            acts.push(Action::FileView {
                path,
                rev: Some(sample.sample.hash.clone()),
                tab: FileTab::File,
            });
        }
        if ui.small_button("Close").on_hover_text("Esc").clicked() {
            open = false;
        }
    });
    if commits.is_empty() {
        common::empty(ui, "No commit of this window changed it");
        return open;
    }
    egui::ScrollArea::vertical()
        .id_salt("insights-churn-window")
        .max_height(150.0)
        .show(ui, |ui| {
            for (c, paths) in &commits {
                let Some(commit) = built.history.commits.get(*c) else {
                    continue;
                };
                let lines: u64 = commit
                    .files
                    .iter()
                    .filter(|f| paths.contains(&f.path))
                    .map(|f| u64::from(f.lines()))
                    .sum();
                let resp = common::table_row(
                    ui,
                    &[90.0, 90.0, 80.0],
                    &[
                        short(&commit.hash).to_owned(),
                        day(commit.time),
                        format!("{lines} lines"),
                        commit.subject.clone(),
                    ],
                    false,
                    false,
                )
                .on_hover_text("Select the commit");
                if resp.clicked() {
                    acts.push(Action::SelectHash(commit.hash.clone()));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(96.0);
                    for path in paths {
                        if ui
                            .small_button(path)
                            .on_hover_text("Open this file's diff in the commit")
                            .clicked()
                        {
                            acts.push(Action::OpenDiff(DiffKey::Commit {
                                hash: commit.hash.clone(),
                                path: path.clone(),
                            }));
                        }
                    }
                });
            }
        });
    open
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}
