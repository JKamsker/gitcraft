//! Pull-request metadata, status checks, and commit-list sections.

use super::*;

pub(super) fn section(ui: &mut Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(22.0);
    ui.label(
        RichText::new(title)
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.add_space(6.0);
}

/// Title, state, who wants what merged where, and how things stand.
pub(super) fn header(ui: &mut Ui, d: &PrDetails) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&d.title).font(theme::semibold(19.0)));
        ui.label(
            RichText::new(format!("#{}", d.number))
                .font(egui::FontId::proportional(19.0))
                .color(t.text_faint),
        );
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        let (state, color) = match (d.state.as_str(), d.draft) {
            ("MERGED", _) => (
                crate::i18n::text("repo-workflows-pr_view-header-bd0a0620"),
                t.accent_fg,
            ),
            ("CLOSED", _) => (
                crate::i18n::text("repo-workflows-pr_view-header-c21ead06"),
                t.danger,
            ),
            (_, true) => (
                crate::i18n::text("repo-workflows-pr_view-header-ebf12ef4"),
                t.text_dim,
            ),
            _ => (
                crate::i18n::text("repo-workflows-pr_view-header-ed077f3d"),
                t.success,
            ),
        };
        widgets::chip(ui, state, color);
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-pr-merge-intention",
                &[
                    ("author", (d.author.as_str()).into()),
                    (
                        "closed",
                        (if matches!(d.state.as_str(), "MERGED" | "CLOSED") {
                            "yes"
                        } else {
                            "no"
                        })
                        .into(),
                    ),
                    ("head", (d.head_branch.as_str()).into()),
                    ("base", (d.base_branch.as_str()).into()),
                ],
            ))
            .color(t.text_dim),
        );
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| stats(ui, d));
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| chips(ui, d));
}

/// When it was opened and updated, and how much it changes.
pub(super) fn stats(ui: &mut Ui, d: &PrDetails) {
    let t = Tokens::get(ui.ctx());
    let now = git::now();
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-pr_view-stats-78774f9f",
            &[
                (
                    "git-relative-time-d-created-at-n-1",
                    (git::relative_time(d.created_at, now)).clone().into(),
                ),
                (
                    "git-relative-time-d-updated-at-n-2",
                    (git::relative_time(d.updated_at, now)).clone().into(),
                ),
            ],
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.label(
        RichText::new(format!("+{}", d.additions))
            .color(t.success)
            .small(),
    );
    ui.label(
        RichText::new(format!("−{}", d.deletions))
            .color(t.danger)
            .small(),
    );
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-pr-file-count",
            &[("count", (d.changed_files).into())],
        ))
        .color(t.text_faint)
        .small(),
    );
}

/// How the reviews and the merge stand, and the labels.
fn chips(ui: &mut Ui, d: &PrDetails) {
    let t = Tokens::get(ui.ctx());
    let label = pr::reviews_label(&d.review_decision, d.state == "OPEN", &d.timeline);
    let color = match label {
        "Approved" => t.success,
        "Changes requested" => t.danger,
        "Review required" => t.warning,
        _ => t.text_dim,
    };
    widgets::chip(
        ui,
        match label {
            "Approved" => crate::i18n::text("repo-workflows-pr_view-chips-87b42e40"),
            "Changes requested" => crate::i18n::text("repo-workflows-pr_view-chips-10a92a8a"),
            "Review required" => crate::i18n::text("repo-workflows-pr_view-chips-f0742e3a"),
            other => other,
        },
        color,
    );
    // Whether it can be merged only means something while it can still be.
    if d.state == "OPEN" {
        match d.mergeable.as_str() {
            "MERGEABLE" => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-pr_view-chips-e72e14ae"),
                t.success,
            ),
            "CONFLICTING" => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-pr_view-chips-dac5ae00"),
                t.danger,
            ),
            _ => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-pr_view-chips-ab0fbfd3"),
                t.text_dim,
            ),
        };
        if d.merge_state == "BEHIND" {
            widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-pr_view-chips-e62d2df5"),
                t.warning,
            );
        }
    }
    for label in &d.labels {
        widgets::chip(ui, label, t.accent_fg);
    }
}

pub(super) fn checks(ui: &mut Ui, d: &PrDetails, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    if let Some(e) = &d.checks_error {
        ui.colored_label(
            t.warning,
            crate::i18n::message(
                "repo-workflows-pr_view-checks-3afaa0c5",
                &[("e", (e).clone().into())],
            ),
        );
        return;
    }
    if d.checks.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-pr_view-checks-7845db2e"))
                .color(t.text_faint)
                .italics(),
        );
        return;
    }
    let (passed, failed, running, skipped) = pr::tally(&d.checks);
    let summary: Vec<String> = [
        (failed, "failed"),
        (running, "running"),
        (passed, "passed"),
        (skipped, "skipped"),
    ]
    .iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, word)| {
        crate::i18n::message(
            "repo-workflows-pr-check-tally",
            &[("count", (*n).into()), ("state", (*word).into())],
        )
    })
    .collect();
    ui.label(RichText::new(summary.join(" · ")).color(t.text_dim));
    ui.add_space(4.0);
    // What needs attention first.
    let rank = |state: CheckState| match state {
        CheckState::Failed => 0,
        CheckState::Running => 1,
        CheckState::Passed => 2,
        CheckState::Skipped => 3,
    };
    let mut sorted: Vec<_> = d.checks.iter().collect();
    sorted.sort_by_key(|c| rank(c.state));
    for check in sorted {
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        crate::repo::widgets::a11y(&r, egui::WidgetType::Button, || {
            let state = crate::repo::ci_status::state_word(check.state);
            crate::a11y::labels::join([check.name.as_str(), check.detail.as_str(), state])
        });
        // A run of GitHub Actions opens in the pipeline view, anything else in the browser.
        // Forgejo's and Gitea's Actions have the same URLs, but gitcraft doesn't read their runs.
        let run = (d.forge == Some(crate::forge::Forge::GitHub))
            .then(|| crate::ci::github::run_of_url(&check.url))
            .flatten();
        let opens = pr::is_web_url(&check.url);
        if r.hovered() && opens {
            ui.painter().rect_filled(rect, t.radius_sm, t.hover);
        }
        let y = rect.center().y;
        let icon = egui::Rect::from_center_size(pos2(rect.left() + 12.0, y), vec2(14.0, 14.0));
        match check.state {
            CheckState::Passed => icons::paint(ui, icon, "check", 13.0, t.success),
            CheckState::Failed => icons::paint(ui, icon, "x", 13.0, t.danger),
            CheckState::Skipped => icons::paint(ui, icon, "minus", 13.0, t.text_faint),
            CheckState::Running => {
                crate::spinner::Spinner::new().size(11.0).paint_at(ui, icon);
            }
        }
        let painter = ui.painter_at(rect);
        let name = painter.text(
            pos2(rect.left() + 26.0, y),
            Align2::LEFT_CENTER,
            &check.name,
            egui::FontId::proportional(12.5),
            t.text,
        );
        painter.text(
            pos2(name.right() + 8.0, y),
            Align2::LEFT_CENTER,
            &check.detail,
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
        if !opens {
            continue;
        }
        let r = match run {
            Some(_) => r.on_hover_text(crate::i18n::message(
                "repo-workflows-pr_view-checks-8432e03f",
                &[("check-url-1", (check.url).clone().into())],
            )),
            None => r.on_hover_text(&check.url),
        };
        if r.clicked() {
            acts.push(match run {
                Some(run) => Action::OpenCi {
                    sha: d.head_oid.clone(),
                    run: Some(run),
                },
                None => Action::OpenUrl(check.url.clone()),
            });
        }
    }
}

/// The commits, those without their author's sign-off marked (`signoffs`: as read here).
pub(super) fn commits(ui: &mut Ui, d: &PrDetails, signoffs: Option<&dco::PrSignoffs>) {
    let t = Tokens::get(ui.ctx());
    for commit in &d.commits {
        ui.horizontal(|ui| {
            if dco::unsigned(signoffs, &commit.oid) {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-pr_view-commits-36f7d679"))
                        .color(t.warning)
                        .small(),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-pr_view-commits-a8da9e62"));
            }
            ui.label(
                RichText::new(short(&commit.oid))
                    .font(theme::mono(11.0))
                    .color(t.text_faint),
            );
            ui.label(&commit.headline);
            ui.label(RichText::new(&commit.author).color(t.text_faint).small());
        });
    }
}
