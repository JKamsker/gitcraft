use super::*;

pub(super) fn route_caption(plan: &Plan) -> String {
    let source = if plan.deleting {
        crate::i18n::text("repo-core-3134")
    } else {
        &plan.branch
    };
    let targets = plan
        .targets
        .iter()
        .map(|label| check::display::target_label(label))
        .collect::<Vec<_>>()
        .join(", ");
    crate::i18n::message(
        "interaction-guards-checklist-route",
        &[
            ("source", source.into()),
            ("destination", plan.destination.as_str().into()),
            ("targets", targets.into()),
        ],
    )
}

pub(in crate::repo) fn dialog(
    ui: &mut egui::Ui,
    plan: &mut Plan,
    acknowledge: &mut bool,
) -> Outcome {
    ui.set_width(640.0);
    ui.heading(crate::i18n::text("repo-core-3132"));
    ui.label(route_caption(plan));
    if plan.deleting {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).warning,
            crate::i18n::message(
                "repo-core-3135",
                &[
                    ("destination", plan.destination.to_string().into()),
                    (
                        "super",
                        crate::repo::short(plan.base.as_deref().unwrap_or(""))
                            .to_string()
                            .into(),
                    ),
                ],
            ),
        );
    }
    for (before, step) in plan.local_steps() {
        ui.label(crate::i18n::message(
            "repo-core-3136",
            &[
                (
                    "After",
                    (if before {
                        crate::i18n::text("repo-core-1564")
                    } else {
                        crate::i18n::text("repo-core-1565")
                    })
                    .to_string()
                    .into(),
                ),
                ("step", step.to_string().into()),
            ],
        ));
    }
    ui.label(crate::i18n::message(
        "repo-core-3137",
        &[
            ("count", (plan.commits.len()).into()),
            (
                "count2",
                (plan.commits.iter().filter(|c| c.signed).count()).into(),
            ),
            ("head", crate::repo::short(&plan.head).to_string().into()),
            ("expected-name", plan.expected_name.to_string().into()),
            ("expected-email", plan.expected_email.to_string().into()),
        ],
    ));
    if plan.skip_hooks {
        ui.label(crate::i18n::text("repo-core-3138"));
    }
    for (host, account) in &plan.expected_accounts {
        ui.label(crate::i18n::message(
            "repo-core-3139",
            &[
                ("host", host.to_string().into()),
                ("account", account.to_string().into()),
            ],
        ));
    }
    ui.label(crate::i18n::text("repo-core-3140"));
    if !plan.lfs_objects.is_empty() {
        ui.label(crate::i18n::message(
            "repo-core-3141",
            &[
                ("count", (plan.lfs_objects.len()).into()),
                (
                    "plan",
                    plan.lfs_targets()
                        .iter()
                        .map(|label| check::display::target_label(label))
                        .collect::<Vec<_>>()
                        .join(", ")
                        .into(),
                ),
            ],
        ));
    }
    if let Err(reason) = check::push_args(plan) {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).warning,
            check::display::diagnostic(&reason),
        );
    }
    let mut out = Outcome::Open;
    if !plan.deleting && !plan.is_current_branch() {
        ui.label(crate::i18n::text("repo-core-3142"));
        if let Some(name) = plan.branch.strip_prefix("refs/heads/")
            && ui.button(crate::i18n::text("repo-core-3143")).clicked()
        {
            out = Outcome::Done(Box::new(Action::Checkout(name.to_owned())));
        }
    }
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .show(ui, |ui| {
            for (kind, title, fix, label) in [
                (
                    Kind::Transcript,
                    crate::i18n::text("repo-core-3144"),
                    "",
                    "",
                ),
                (
                    Kind::Identity,
                    crate::i18n::text("repo-core-3145"),
                    "author",
                    crate::i18n::text("repo-core-3146"),
                ),
                (
                    Kind::Signature,
                    crate::i18n::text("repo-core-3147"),
                    "sign",
                    crate::i18n::text("repo-core-3148"),
                ),
                (
                    Kind::Message,
                    crate::i18n::text("repo-core-3149"),
                    "messages",
                    crate::i18n::text("repo-core-3151"),
                ),
                (
                    Kind::Secret,
                    crate::i18n::text("repo-core-3152"),
                    "secrets",
                    crate::i18n::text("repo-core-3153"),
                ),
                (
                    Kind::Large,
                    crate::i18n::text("repo-core-3154"),
                    "lfs",
                    crate::i18n::text("repo-core-3155"),
                ),
                (
                    Kind::RemoteHistory,
                    crate::i18n::text("repo-core-3156"),
                    "remote",
                    crate::i18n::text("repo-core-3157"),
                ),
            ] {
                let found: Vec<_> = plan.findings.iter().filter(|f| f.kind == kind).collect();
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong(format!(
                        "{} {title} — {}",
                        if found.is_empty() {
                            "OK"
                        } else {
                            crate::i18n::text("repo-core-3160")
                        },
                        if found.is_empty() {
                            crate::i18n::text("repo-core-3161").into()
                        } else {
                            crate::i18n::message(
                                "repo-core-3162",
                                &[("count", (found.len()).into())],
                            )
                        }
                    ));
                    if !found.is_empty()
                        && !fix.is_empty()
                        && plan.is_current_branch()
                        && ui.small_button(label).clicked()
                    {
                        out = Outcome::Done(Box::new(if fix == "messages" {
                            Action::PreflightMessages(Box::new(plan.clone()))
                        } else {
                            Action::FixPreflight(Box::new(plan.clone()), fix.into())
                        }));
                    }
                });
                for f in found.iter().take(100) {
                    ui.label(format!(
                        "{} {} {}",
                        crate::repo::short(&f.commit),
                        check::display::path(&f.path),
                        check::display::diagnostic(&f.detail)
                    ));
                }
                if found.len() > 100 {
                    ui.label(crate::i18n::text("repo-core-3163"));
                }
            }
        });
    if !plan.deleting && !plan.destination.starts_with("refs/for/") {
        ui.add_enabled_ui(plan.base.is_some(), |ui| {
            ui.checkbox(&mut plan.force, crate::i18n::text("repo-core-3165"));
        });
        if plan.base.is_none() {
            ui.label(crate::i18n::text("repo-core-3166"));
        }
    }
    ui.checkbox(acknowledge, crate::i18n::text("repo-core-3167"));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                *acknowledge && check::push_args(plan).is_ok(),
                egui::Button::new(if plan.deleting {
                    crate::i18n::text("repo-core-3168")
                } else if plan.force {
                    crate::i18n::text("repo-core-3169")
                } else {
                    crate::i18n::text("repo-core-3170")
                }),
            )
            .clicked()
        {
            out = Outcome::Done(Box::new(Action::SubmitPreflight(Box::new(plan.clone()))));
        }
        if ui.button(crate::i18n::text("repo-core-3171")).clicked() {
            out = Outcome::Done(Box::new(Action::ReloadPreflight(Box::new(plan.clone()))));
        }
        if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
            out = Outcome::Cancel;
        }
    });
    out
}
