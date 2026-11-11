//! My-work repository actions and machine discovery controls.

use super::*;

/// One repository: its name, Open, and a row per piece of work.
pub(super) fn repo_block(
    ui: &mut Ui,
    w: &MyWork,
    repo: &Path,
    work: &[Work],
    t: &Tokens,
) -> Option<Ask> {
    let mut ask = None;
    egui::Frame::new()
        .fill(t.dock)
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(super::super::workspaces::repo_name(repo))
                        .font(theme::semibold(13.0)),
                );
                ui.label(
                    RichText::new(crate::remote::display(repo))
                        .color(t.text_faint)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(crate::i18n::text("interaction-my-work-open"))
                        .clicked()
                    {
                        ask = Some(Ask::Open(repo.to_owned()));
                    }
                });
            });
            for item in work {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    icons::inline(ui, item.icon(), 13.0, t.icon);
                    ui.label(item.describe());
                    if let Some((branch, up)) = item.push() {
                        if let Some(a) = push_button(ui, w, repo, branch, up, t) {
                            ask = Some(a);
                        }
                    } else if let Some(why) = item.why_no_push() {
                        ui.label(
                            RichText::new(crate::i18n::text("interaction-my-work-open-push"))
                                .color(t.text_faint),
                        )
                        .on_hover_text(why);
                    }
                });
            }
        });
    ui.add_space(4.0);
    ask
}

/// Push for one branch, or how its push went.
pub(super) fn push_button(
    ui: &mut Ui,
    w: &MyWork,
    repo: &Path,
    branch: &str,
    has_upstream: bool,
    t: &Tokens,
) -> Option<Ask> {
    let key = (repo.to_owned(), branch.to_owned());
    let ask = || Ask::Push(repo.to_owned(), branch.to_owned(), has_upstream);
    // Asking another machine connects to it, which may ask for a password: only when said so.
    let far = crate::remote::parse(repo)
        .map(|t| t.host)
        .filter(|h| !crate::remote::is_connected(h));
    let label = if far.is_some() {
        crate::i18n::text("interaction-my-work-connect-push")
    } else {
        crate::i18n::text("interaction-my-work-push")
    };
    match w.pushes.get(&key) {
        Some(None) => {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            let queued = w.push_queue.iter().any(|(k, _)| k == &key);
            let text = if queued {
                crate::i18n::text("interaction-my-work-waiting")
            } else {
                crate::i18n::text("interaction-my-work-pushing")
            };
            ui.label(RichText::new(text).color(t.text_faint));
            None
        }
        Some(Some(Err(e))) => {
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-my-work-push-error",
                    &[("reason", across::display_reason(e).into())],
                ))
                .color(t.danger),
            )
            .on_hover_text(across::display_error(e));
            ui.button(label)
                .on_hover_text(crate::i18n::text("interaction-my-work-retry"))
                .clicked()
                .then(ask)
        }
        _ => {
            let tracked = if has_upstream { "yes" } else { "no" };
            let hint = match &far {
                Some(host) => crate::i18n::message(
                    "interaction-my-work-push-remote",
                    &[("host", host.label().into()), ("tracked", tracked.into())],
                ),
                None => crate::i18n::message(
                    "interaction-my-work-push-local",
                    &[("tracked", tracked.into())],
                ),
            };
            ui.button(label).on_hover_text(hint).clicked().then(ask)
        }
    }
}

/// The machines whose home folders can be searched for repositories gitcraft doesn't know yet.
pub(super) fn machines_line(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.label(
        RichText::new(crate::i18n::text("interaction-my-work-find"))
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.label(
        RichText::new(crate::i18n::message(
            "interaction-my-work-search-description",
            &[("depth", discover::DEPTH.into())],
        ))
        .color(t.text_faint)
        .small(),
    );
    let Some(machines) = &w.machines else {
        ui.add(crate::spinner::Spinner::new().size(12.0));
        return None;
    };
    ui.horizontal_wrapped(|ui| {
        for machine in machines {
            match w.searches.get(machine) {
                Some(None) => {
                    ui.add(crate::spinner::Spinner::new().size(12.0));
                    ui.label(
                        RichText::new(format!("{}…", machine.display_label())).color(t.text_dim),
                    );
                }
                state => {
                    let label = match state {
                        Some(Some(Ok((n, true)))) => crate::i18n::message(
                            "interaction-my-work-found-cut",
                            &[
                                ("machine", machine.display_label().into()),
                                ("count", (*n).into()),
                            ],
                        ),
                        Some(Some(Ok((n, false)))) => crate::i18n::message(
                            "interaction-my-work-found",
                            &[
                                ("machine", machine.display_label().into()),
                                ("count", (*n).into()),
                            ],
                        ),
                        Some(Some(Err(_))) => crate::i18n::message(
                            "interaction-my-work-search-failed",
                            &[("machine", machine.display_label().into())],
                        ),
                        _ => machine.display_label(),
                    };
                    let mut b = ui.button(label);
                    if let Some(Some(Err(e))) = state {
                        b = b.on_hover_text(across::display_error(e));
                    }
                    if b.clicked() {
                        ask = Some(Ask::Search(machine.clone()));
                    }
                }
            }
        }
    });
    ask
}
