//! Review heat controls, seen-line annotations, and thread details.

use super::*;

/// The diff line (by its place) showing line `n` of the old side (`old`) or the new one; the
/// nearest when the diff doesn't show it.
/// The line above the grid: which pull request (a menu of the others), opening it, and every
/// file's hunks shown or hidden.
pub(super) fn bar(ui: &mut Ui, state: &State, prs: &[(u64, String)], acts: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .number
            .map(|n| {
                prs.iter()
                    .find(|(m, _)| *m == n)
                    .map_or_else(|| format!("#{n}"), |(_, title)| format!("#{n} {title}"))
            })
            .unwrap_or_else(|| "Pick a pull request".to_owned());
        egui::ComboBox::from_id_salt("review-heat-pr")
            .selected_text(shown)
            .width(360.0)
            .show_ui(ui, |ui| {
                for (n, title) in prs {
                    let on = state.number == Some(*n);
                    if ui.selectable_label(on, format!("#{n} {title}")).clicked() && !on {
                        acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Pick(*n))));
                    }
                }
                if prs.is_empty() {
                    ui.label("No pull requests listed");
                }
            });
        if let Some(n) = state.number {
            if ui
                .button(format!("Open #{n}"))
                .on_hover_text("Its page: description, checks, conversation")
                .clicked()
            {
                acts.push(Action::OpenPr(n));
            }
            if state.data.ready().is_some() {
                if ui
                    .button("Expand all")
                    .on_hover_text("Every file's hunks")
                    .clicked()
                {
                    acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::ExpandAll(
                        true,
                    ))));
                }
                if ui.button("Collapse all").clicked() {
                    acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::ExpandAll(
                        false,
                    ))));
                }
            }
        }
    });
}

/// What the version seen in gitcraft is, and when it is stale, why, with the way to compare.
pub(super) fn seen_line(
    ui: &mut Ui,
    data: &Heat,
    pr: Option<&crate::forge::PullRequest>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let text = match &data.seen {
        Some(Seen::Fresh { tip, .. }) if *tip == data.head => format!(
            "Seen in gitcraft at {}, the head now: nothing changed since.",
            short(tip)
        ),
        Some(Seen::Fresh { tip, since }) => format!(
            "Seen in gitcraft at {}; {} file(s) changed since. gitcraft remembers the version \
             looked at, not which lines were read.",
            short(tip),
            since.len()
        ),
        Some(Seen::Never) => "Not seen in gitcraft yet: opening its page, comparing its versions \
                              or marking it as seen keeps a version to count changes from."
            .to_owned(),
        Some(Seen::Unknown(why)) => format!("What was seen in gitcraft is unknown: {why}."),
        Some(Seen::Stale { .. }) | None => String::new(),
    };
    if !text.is_empty() {
        ui.add(egui::Label::new(RichText::new(text).color(t.text_dim)).wrap());
    }
    let Some(why) = &data.stale_review else {
        return;
    };
    egui::Frame::new()
        .fill(t.warning.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, t.warning.gamma_multiply(0.6)))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                crate::repo::widgets::chip(ui, "stale review", t.warning);
                ui.add(egui::Label::new(RichText::new(why).color(t.text)).wrap());
                if let Some(pr) = pr
                    && ui
                        .button("Compare the versions")
                        .on_hover_text(
                            "The commits seen and the commits now, paired up, in Rebase ribbons",
                        )
                        .clicked()
                {
                    let source = crate::insights::ribbons::Source::Pr {
                        number: pr.number,
                        title: pr.title.clone(),
                        head: pr.sha.clone(),
                        base: pr.base.clone(),
                    };
                    acts.push(Action::Insights(InsightsAct::Open(Kind::Ribbons)));
                    acts.push(Action::Insights(InsightsAct::Ribbons(
                        crate::repo::insights::ribbons::Act::Show(source),
                    )));
                }
            });
        });
}

pub(super) fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (Mark::Glyph("=", t.success), "as seen in gitcraft"),
            (Mark::Glyph("~", t.warning), "changed since seen"),
            (
                Mark::Hollow(t.success),
                "same content after a rewrite (inferred)",
            ),
            (Mark::Glyph("!", t.danger), "seen version rewritten: stale"),
            (Mark::Glyph("·", t.text_faint), "not seen here"),
            (Mark::Fill(t.accent), "lines changed since (darker: more)"),
            (Mark::Hatched(t.warning), "open review threads"),
            (Mark::Glyph("?", t.text_faint), "unknown"),
        ],
    );
}

/// How many threads are listed at most.
const MAX_THREADS: usize = 60;

/// The review threads, open ones first: each opens where it is, or its comment on the forge.
pub(super) fn threads(ui: &mut Ui, data: &Heat, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let Some(all) = &data.threads else {
        common::heading(ui, "Review threads");
        ui.label(
            RichText::new("The review threads couldn't be read: unknown, not none.")
                .color(t.text_faint),
        );
        return;
    };
    let open = all.iter().filter(|th| !th.resolved).count();
    common::heading(ui, &format!("Review threads: {open} open of {}", all.len()));
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by_key(|&i| (all[i].resolved, all[i].path.clone(), all[i].line));
    egui::ScrollArea::vertical()
        .id_salt("review-heat-threads")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for &i in order.iter().take(MAX_THREADS) {
                let th = &all[i];
                ui.horizontal(|ui| {
                    let place = match th.line {
                        Some(n) => format!("{}:{n}", th.path),
                        None => th.path.clone(),
                    };
                    let listed = data.files.iter().any(|f| f.file.path == th.path);
                    let link = ui.link(place).on_hover_text(if listed {
                        "Open the file's diff at the pull request's revisions, at this line"
                    } else {
                        "Not among the files read: open its diff at the pull request's \
                         revisions all the same"
                    });
                    if link.clicked() {
                        let at = th.line.map(|n| (th.left, n.min(u32::MAX as u64) as u32));
                        acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Open {
                            key: DiffKey::Range {
                                from: data.from.clone(),
                                to: Some(data.head.clone()),
                                path: th.path.clone(),
                            },
                            at,
                            cell: None,
                        })));
                    }
                    let state = match (th.resolved, th.outdated) {
                        (true, _) => "resolved",
                        (false, true) => "open, outdated",
                        (false, false) => "open",
                    };
                    ui.label(RichText::new(state).color(if th.resolved {
                        t.text_faint
                    } else {
                        t.warning
                    }));
                    if crate::pull_request::is_web_url(&th.url)
                        && ui
                            .small_button("on the forge")
                            .on_hover_text(th.url.clone())
                            .clicked()
                    {
                        acts.push(Action::OpenUrl(th.url.clone()));
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{}: {}", th.author, th.text)).color(t.text_dim),
                        )
                        .truncate(),
                    );
                });
            }
            if all.len() > MAX_THREADS {
                ui.label(
                    RichText::new(format!("… and {} more", all.len() - MAX_THREADS))
                        .color(t.text_faint),
                );
            }
        });
}

/// What the mark of a file or hunk's state seen says, for cells and tooltips.
pub(in crate::repo::insights) fn seen_mark(mark: SeenMark, t: &Tokens) -> (egui::Color32, Mark) {
    match mark {
        SeenMark::Same => (common::tint(t.success, 0.3, t), Mark::Glyph("=", t.success)),
        SeenMark::Changed => (
            common::tint(t.warning, 0.35, t),
            Mark::Glyph("~", t.warning),
        ),
        SeenMark::SameAfterRewrite => (t.field, Mark::Hollow(t.success)),
        SeenMark::Stale => (common::tint(t.danger, 0.25, t), Mark::Glyph("!", t.danger)),
        SeenMark::NotSeen => (t.field, Mark::Glyph("·", t.text_faint)),
        SeenMark::Unknown => (t.field, Mark::Glyph("?", t.text_faint)),
    }
}
