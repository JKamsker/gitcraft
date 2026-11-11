use super::*;

pub(super) fn grid(
    ui: &mut Ui,
    view: &Bakeoff,
    cmp: &Comparison,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    egui::Grid::new("bakeoff-grid")
        .striped(true)
        .num_columns(cmp.attempts.len() + 1)
        .spacing([18.0, 6.0])
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label("");
            for a in &view.attempts {
                ui.label(RichText::new(&a.name).font(theme::semibold(12.5)));
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-ce199c9e"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.label(crate::i18n::number(c.files.len()));
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-1af85190"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.horizontal(|ui| {
                    ui.colored_label(t.success, format!("+{}", c.added));
                    ui.colored_label(t.danger, format!("−{}", c.removed));
                });
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-9c37d9c9"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                if c.new_deps.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-140bedbf"))
                            .color(t.text_faint),
                    );
                } else {
                    ui.vertical(|ui| {
                        for dep in &c.new_deps {
                            ui.colored_label(t.warning, dep);
                        }
                    });
                }
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-7a44bde0"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.label(if c.uncommitted {
                    crate::i18n::text("repo-workflows-bakeoff-grid-1f19288a")
                } else {
                    crate::i18n::text("repo-workflows-paint-no")
                });
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-6a3fe887"))
                    .color(t.text_dim),
            );
            for (i, a) in view.attempts.iter().enumerate() {
                let built = view.builds.get(i).copied().flatten();
                ui.vertical(|ui| {
                    build_cell(ui, built, a.worktree.is_some(), i, t, act);
                    if view.statuses.get(i).is_some_and(Option::is_some)
                        && ui
                            .small_button(crate::i18n::text("repo-workflows-bakeoff-grid-7cbda0dd"))
                            .on_hover_text(crate::i18n::text(
                                "repo-workflows-bakeoff-grid-6263bbf3",
                            ))
                            .clicked()
                    {
                        *act = Some(BakeAct::Output(i));
                    }
                });
            }
            ui.end_row();
        });
}

pub(super) fn build_cell(
    ui: &mut Ui,
    built: Option<Built>,
    _has_worktree: bool,
    i: usize,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        match built {
            Some(Built::Preparing) => {
                ui.label(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-98cd8a0b",
                ));
            }
            Some(Built::Unavailable) => {
                ui.colored_label(
                    t.warning,
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-5dac180d"),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-63f96d79",
                ));
            }
            Some(Built::Running) => {
                ui.add(crate::spinner::Spinner::new().size(12.0));
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bakeoff-build-cell-899686f3",
                    ))
                    .color(t.text_dim),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-ef24cfb5",
                ));
            }
            Some(Built::Passed) => {
                ui.colored_label(
                    t.success,
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-d07e6c18"),
                );
            }
            Some(Built::Failed(code)) => {
                ui.colored_label(
                    t.danger,
                    crate::i18n::message(
                        "repo-workflows-bakeoff-build-cell-301c453c",
                        &[("code", (code).into())],
                    ),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-ef24cfb5",
                ));
            }
            None => {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bakeoff-build-cell-26c16489",
                    ))
                    .color(t.text_faint),
                );
            }
        }
        if built != Some(Built::Preparing) {
            let (label, tip) = match built {
                Some(Built::Running) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-6fe4abd4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-17824e4b"),
                ),
                Some(Built::Unavailable) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-6fe4abd4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-84b604d3"),
                ),
                Some(_) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-af81e9e4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-adccacb7"),
                ),
                None => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-bdd254b6"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-3149d8b3"),
                ),
            };
            if ui.small_button(label).on_hover_text(tip).clicked() {
                *act = Some(BakeAct::Build(i));
            }
        }
    });
}

// Every file an attempt touched: who touched it, what is picked of it.
