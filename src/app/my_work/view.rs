//! My-work panel header and grouped machine results.

use super::*;

/// The title, what the panel does, how far the look is, and Look again.
pub(super) fn panel_header(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text("interaction-my-work-title"))
                .font(theme::semibold(16.0)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", crate::i18n::text("interaction-my-work-close"))
                .clicked()
            {
                ask = Some(Ask::Close);
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("interaction-my-work-look"),
            )
            .clicked()
            {
                ask = Some(Ask::LookAll);
            }
        });
    });
    ui.label(RichText::new(crate::i18n::text("interaction-my-work-intro")).color(t.text_dim));
    if w.busy() {
        let (total, done) = w.round;
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-my-work-progress",
                    &[("done", done.into()), ("total", total.into())],
                ))
                .color(t.text_faint),
            );
        });
    }
    ui.add_space(8.0);
    ask
}

/// The repositories of one machine, sorted out by what their looks said.
#[derive(Default)]
struct MachineRows<'a> {
    with_work: Vec<(&'a PathBuf, Vec<Work>)>,
    unlooked: Vec<PathBuf>,
}

/// The repositories with work only there, by machine; then those with none, those not looked
/// at and those that couldn't be.
pub(super) fn results(ui: &mut Ui, w: &MyWork, found: &[PathBuf], t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    let mut machines: Vec<(String, MachineRows)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut clean = 0;
    let mut unlooked_in_all = 0;
    let mut failed: Vec<(&PathBuf, &String)> = Vec::new();
    for p in &w.looked {
        let m = my_work::machine_of(p);
        let at = *index.entry(m.clone()).or_insert_with(|| {
            machines.push((m, MachineRows::default()));
            machines.len() - 1
        });
        let rows = &mut machines[at].1;
        match w.seen.get(p) {
            Some(Ok(o)) => {
                let work = my_work::work_of(o);
                if work.is_empty() {
                    clean += 1;
                } else {
                    rows.with_work.push((p, work));
                }
            }
            Some(Err(e)) => failed.push((p, e)),
            None if !w.pending.contains(p) => {
                unlooked_in_all += 1;
                rows.unlooked.push(p.clone());
            }
            None => {}
        }
    }
    let mut any_work = false;
    for (machine, rows) in &machines {
        if rows.with_work.is_empty() && rows.unlooked.is_empty() {
            continue;
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(if machine == "This computer" {
                crate::i18n::text("interaction-my-work-this-computer").to_uppercase()
            } else {
                machine.to_uppercase()
            })
            .font(theme::semibold(10.5))
            .color(t.text_faint),
        );
        if !rows.unlooked.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "interaction-my-work-unlooked",
                        &[("count", rows.unlooked.len().into())],
                    ))
                    .color(t.text_dim),
                );
                if ui
                    .button(crate::i18n::text("interaction-my-work-connect-look"))
                    .clicked()
                {
                    ask = Some(Ask::LookAt(rows.unlooked.clone()));
                }
            });
        }
        for (repo, work) in &rows.with_work {
            any_work = true;
            if let Some(a) = repo_block(ui, w, repo, work, t) {
                ask = Some(a);
            }
        }
    }
    ui.add_space(8.0);
    if !any_work && !w.busy() && !w.looked.is_empty() && failed.is_empty() {
        ui.horizontal(|ui| {
            icons::inline(ui, "check", 14.0, t.success);
            if unlooked_in_all == 0 {
                ui.label(crate::i18n::text("interaction-my-work-clean"));
            } else {
                ui.label(crate::i18n::message(
                    "interaction-my-work-clean-looked",
                    &[("count", clean.into())],
                ));
            }
        });
    }
    if w.looked.is_empty() {
        ui.label(RichText::new(crate::i18n::text("interaction-my-work-empty")).color(t.text_dim));
    }
    if clean > 0 {
        ui.label(
            RichText::new(crate::i18n::message(
                "interaction-my-work-clean-count",
                &[("count", clean.into())],
            ))
            .color(t.text_faint),
        );
    }
    if !failed.is_empty() {
        egui::CollapsingHeader::new(
            RichText::new(crate::i18n::message(
                "interaction-my-work-failed-look",
                &[("count", failed.len().into())],
            ))
            .color(t.danger),
        )
        .id_salt("my-work-failed")
        .show(ui, |ui| {
            for (path, e) in failed {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            crate::remote::display(path),
                            across::display_error(e)
                        ))
                        .color(t.text_dim)
                        .small(),
                    );
                    if found.contains(path)
                        && matches!(
                            across::short_reason(e),
                            "folder not found" | "not a repository"
                        )
                        && icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-my-work-forget"),
                        )
                        .clicked()
                    {
                        ask = Some(Ask::Forget(path.clone()));
                    }
                });
            }
        });
    }
    ask
}
