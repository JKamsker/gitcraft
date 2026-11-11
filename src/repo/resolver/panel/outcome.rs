use super::*;

pub(super) fn result_line(
    t: &Tokens,
    result: &FileResult,
) -> (&'static str, Color32, &'static str) {
    match result {
        FileResult::Resolved => ("check", t.success, "resolved"),
        FileResult::Deleted => ("minus", t.success, "deleted, as one side did"),
        FileResult::StillConflicted => ("circle-alert", t.warning, "still has conflicts"),
        FileResult::ChangedMeanwhile => (
            "circle-alert",
            t.warning,
            "you changed it meanwhile: kept yours",
        ),
        FileResult::Untouched => ("circle-alert", t.warning, "not decided: yours to choose"),
        FileResult::NotPlain => ("x", t.danger, "a link, not a file: not applied"),
    }
}

pub(super) fn outcome(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let summary = run
        .progress
        .finished
        .as_ref()
        .map(|f| f.summary.clone())
        .or_else(|| run.last_words.clone())
        .filter(|s| !s.is_empty());
    if let Some(summary) = summary {
        egui::Frame::new()
            .fill(t.accent.gamma_multiply(0.08))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(summary).color(t.text)).wrap());
            });
        ui.add_space(6.0);
    }
    if let Some(e) = &run.error {
        ui.add(egui::Label::new(RichText::new(e).color(t.warning).small()).wrap());
    }
    for (i, (path, result)) in run.results.iter().enumerate() {
        let (icon, color, words) = result_line(t, result);
        ui.horizontal(|ui| {
            let mut on = run.picked.contains(path);
            let usable = result.applies() && run.phase == Phase::Ready;
            let pick = ui.add_enabled(usable, egui::Checkbox::without_text(&mut on));
            name_pick(&pick, path, on);
            if pick.changed() {
                acts.push(Action::Resolver(ResolverAct::Pick(path.clone(), on)));
            }
            icons::inline(ui, icon, 13.0, color);
            let name = ui
                .add(
                    egui::Label::new(RichText::new(path).font(theme::mono(11.5)))
                        .sense(Sense::click()),
                )
                .on_hover_text("See what the agent did to it")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.label(RichText::new(words).color(t.text_faint).small());
            let review = ui.add_enabled(
                run.phase == Phase::Ready,
                egui::Button::new(RichText::new("Review").small()).small(),
            );
            if name.clicked() || review.clicked() {
                acts.push(Action::Resolver(ResolverAct::Review(i)));
            }
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                run.phase == Phase::Ready && !run.results.is_empty(),
                egui::Button::new("Review changes"),
            )
            .on_hover_text("Each conflict with its two sides and what the agent wrote in its place")
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Review(0)));
        }
        // The merge put aside: brought back first, then the files are applied to it.
        if let Some(parked) = &run.parked {
            bring_back_button(ui, parked, acts);
            return;
        }
        let n = run.picked.len();
        let label = match n {
            0 => "Nothing to apply".to_owned(),
            1 => "Apply 1 file".to_owned(),
            n => format!("Apply {n} files"),
        };
        let can = n > 0 && run.phase == Phase::Ready;
        if widgets::primary_button(ui, &label, can, 0.0)
            .on_hover_text(
                "Writes the agent's version of each file picked here and marks it resolved",
            )
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Apply));
        }
        if run.phase == Phase::Applying {
            ui.add(crate::spinner::Spinner::new().size(14.0));
        }
        if ui
            .add_enabled(run.phase == Phase::Ready, egui::Button::new("Discard"))
            .on_hover_text("Forget the agent's work; your checkout stays as it is")
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
        }
    });
}

/// Names a file's _apply it_ checkbox by its file, for screen readers.
pub(in crate::repo::resolver) fn name_pick(resp: &egui::Response, path: &str, on: bool) {
    let enabled = resp.enabled();
    resp.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            enabled,
            on,
            crate::a11y::labels::name(&format!("Apply {path}")),
        )
    });
}

/// _Bring it back_: the merge put aside, back in the worktree as it was, to apply the agent's
/// work to.
pub(in crate::repo::resolver) fn bring_back_button(
    ui: &mut Ui,
    parked: &crate::resolver::park::Parked,
    acts: &mut Vec<Action>,
) {
    if widgets::primary_button(ui, "Bring it back", true, 0.0)
        .on_hover_text(format!(
            "Checks out {} and puts the {} back as it was, conflicts and staged files \
             included; then the agent's work is there to review and apply",
            parked.branch.as_deref().unwrap_or("its commit"),
            parked.what()
        ))
        .clicked()
    {
        acts.push(Action::Resolver(ResolverAct::BringBack(parked.id.clone())));
    }
    if ui
        .small_button("Drop…")
        .on_hover_text(
            "When it can't come back (its branch moved or is gone): leaves it put aside for good, \
             its files (conflict markers and all) kept on a branch of their own, parked/…",
        )
        .clicked()
    {
        acts.push(Action::Resolver(ResolverAct::DropParked(parked.id.clone())));
    }
}

pub(super) fn applied(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let text = match run.applied {
        0 => "Nothing was applied.".to_owned(),
        1 if run.unstaged => "Wrote 1 file.".to_owned(),
        1 => "Applied 1 file and marked it resolved.".to_owned(),
        n if run.unstaged => format!("Wrote {n} files."),
        n => format!("Applied {n} files and marked them resolved."),
    };
    ui.label(RichText::new(text).color(t.text));
    if let Some(e) = &run.error {
        ui.add(egui::Label::new(RichText::new(e).color(t.warning).small()).wrap());
    }
    ui.label(
        RichText::new("Check the result in the staged changes before you commit.")
            .color(t.text_dim)
            .small(),
    );
    ui.add_space(6.0);
    if ui.button("Close").clicked() {
        acts.push(Action::Resolver(ResolverAct::Dismiss));
    }
}

pub(super) fn failed(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let e = run
        .error
        .as_deref()
        .unwrap_or("The agent ended without resolving anything.");
    ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        // Its merge put aside: brought back before anything else (it is never dropped).
        if let Some(parked) = &run.parked {
            bring_back_button(ui, parked, acts);
            return;
        }
        if ui.button("Try again").clicked() {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
            acts.push(Action::Resolver(ResolverAct::Start(Some(
                run.files.clone(),
            ))));
        }
        if ui.button("Close").clicked() {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
        }
    });
}
