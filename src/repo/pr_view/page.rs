//! The page of the pull request in the big area, and the files it changes in the right pane.

use egui::{Align2, RichText, Sense, Ui, pos2, vec2};

use crate::git;
use crate::icons;
use crate::pull_request::{self as pr, CheckState, PrDetails};
use crate::repo::popout::{self, Panel};
use crate::repo::pr_review::{self, Place, PrDraft};
use crate::repo::{Action, DiffKey, RepoTab, dco, short, widgets};
use crate::theme::{self, Tokens};

use super::timeline::timeline;
use super::{PrView, blocks, only_web_links};

const CONTENT_W: f32 = 900.0;
const MARGIN_X: f32 = 24.0;

impl RepoTab {
    /// The big area.
    pub(in crate::repo) fn pr_view_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let popped = !self.docked(Panel::Pr);
        let Some(view) = &mut self.pr_view else {
            return;
        };
        page_header(ui, view, popped, acts);
        if let Some(relocated) = view.relocations().filter(|m| !m.is_empty()) {
            egui::CollapsingHeader::new(crate::i18n::message(
                "repo-workflows-pr_view-pr-view-ui-c6f6b4c5",
                &[("relocated-len-1", (relocated.len()).clone().into())],
            ))
            .id_salt("pr-relocations")
            .show(ui, |ui| {
                if let Some(Ok(files)) = &view.files {
                    for target in relocated.values() {
                        if ui
                            .button(crate::i18n::message(
                                "repo-workflows-pr_view-pr-view-ui-13197fd2",
                                &[
                                    ("target-path-1", (target.path).clone().into()),
                                    ("target-line-2", (target.line).clone().into()),
                                ],
                            ))
                            .clicked()
                        {
                            acts.push(Action::OpenDiff(DiffKey::Range {
                                from: files.from.clone(),
                                to: Some(files.to.clone()),
                                path: target.path.clone(),
                            }));
                            acts.push(Action::Pr(pr_review::PrAct::OpenLine(target.clone())));
                        }
                    }
                }
            });
        }

        if let Some(error) = &view.relocated.error {
            ui.weak(crate::i18n::message(
                "repo-workflows-pr_view-pr-view-ui-14efff25",
                &[("error", (error).clone().into())],
            ));
        }
        if let Some(Ok(_)) = &view.details {
            crate::repo::versions::since_banner(
                ui,
                self.versions.since.as_ref(),
                view.number,
                acts,
            );
        }
        match &view.details {
            None => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
            }
            Some(Err(e)) => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.colored_label(
                        t.danger,
                        git::display_error(&crate::forge::inbox::display_error(e)),
                    )
                });
            }
            Some(Ok(_)) => {
                // Text wraps to this; only what can't (a table of images) scrolls sideways.
                let width = (ui.available_width() - 2.0 * MARGIN_X).min(CONTENT_W);
                let drafts = &mut self.pr_drafts;
                let dco = Dco {
                    state: &self.dco,
                    current: self.refs.current.as_deref(),
                    idle: self.busy_op.is_none(),
                };
                egui::ScrollArea::both()
                    .id_salt(("pr-view", view.number))
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(MARGIN_X as i8, 18))
                            .show(ui, |ui| {
                                ui.set_max_width(width);
                                let draft = drafts.entry(view.number).or_default();
                                if let Some(Ok(files)) = &view.files {
                                    crate::repo::review_workflow::freshness_ui(
                                        ui,
                                        &mut view.freshness,
                                        files,
                                        acts,
                                    );
                                }
                                if let (Some(Ok(details)), Some(Ok(files))) =
                                    (&view.details, &view.files)
                                {
                                    crate::repo::review_workflow::storyboard_ui(
                                        ui,
                                        &mut view.storyboard,
                                        details,
                                        files,
                                        &self.remotes,
                                        acts,
                                    );
                                }
                                page(ui, view, draft, width, &dco, acts);
                            });
                    });
            }
        }
        only_web_links(ui.ctx());
    }

    /// The right pane: the files the pull request changes.
    pub(in crate::repo) fn pr_files_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        widgets::panel_header(
            ui,
            crate::i18n::text("repo-workflows-pr_view-pr-files-panel-5d4041aa"),
            crate::repo::branch_tree::files_toggle,
        );
        let Some(view) = &self.pr_view else {
            return;
        };
        // What can be done with the pull request's branch here.
        if let Some(Ok(details)) = &view.details {
            let idle = self.busy_op.is_none();
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.pr_worktree_button(
                            ui,
                            acts,
                            details.number,
                            &details.head_branch,
                            &details.head_oid,
                            idle,
                            false,
                        );
                    });
                    if self.has_fork_items(details.number) {
                        ui.horizontal_wrapped(|ui| {
                            self.fork_items(ui, acts, details.number, idle, true);
                        });
                    } else if let Some(e) = self.forks.error() {
                        ui.label(
                            egui::RichText::new(crate::i18n::message(
                                "repo-workflows-pr_view-pr-files-panel-846bc938",
                                &[(
                                    "e-lines-next-unwrap-or-default-1",
                                    (e.lines().next().unwrap_or_default()).clone().into(),
                                )],
                            ))
                            .color(t.text_faint)
                            .small(),
                        );
                    }
                });
            ui.separator();
        }
        if let Some(Ok(details)) = view
            .details
            .as_ref()
            // What's pending stays to send or discard, whatever has become of the pull request.
            .filter(|d| {
                d.as_ref().is_ok_and(|d| {
                    !d.extras.pr_id.is_empty()
                        && (d.state == "OPEN" || d.extras.pending_review.is_some())
                })
            })
        {
            let own = crate::pr_post::is_own(&details.author, details.extras.viewer.as_deref());
            let draft = self.pr_drafts.entry(view.number).or_default();
            egui::Panel::bottom("review_box")
                .frame(
                    egui::Frame::new()
                        .fill(t.card)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .stroke(egui::Stroke::new(1.0, t.separator)),
                )
                .show(ui, |ui| {
                    let open = details.state == "OPEN";
                    pr_review::review_box(ui, draft, &details.extras, own, open, acts);
                });
        }
        match &view.files {
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
            }
            Some(Err(e)) => {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.colored_label(
                            t.danger,
                            crate::i18n::message(
                                "repo-workflows-pr_view-pr-files-panel-80b9e65e",
                                &[("e", (e).clone().into())],
                            ),
                        );
                    });
            }
            Some(Ok(files)) => {
                self.review_all_button(
                    ui,
                    crate::repo::review_workflow::ReviewSource::PullRequest {
                        number: view.number,
                        url: view
                            .details
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .map(|d| d.url.clone())
                            .unwrap_or_default(),
                    },
                    acts,
                );
                crate::repo::detail::files_header(ui, &files.files, None);
                egui::ScrollArea::vertical()
                    .id_salt("pr-files")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.file_list(ui, acts, "pr", &files.files, Some(&files.to), |path| {
                            DiffKey::Range {
                                from: files.from.clone(),
                                to: Some(files.to.clone()),
                                path: path.to_owned(),
                            }
                        });
                    });
            }
        }
    }
}

/// The title bar of the page: its number, and the buttons to leave, open or load it again.
/// `popped`: the page is in its own window (see [`popout::button`]).
fn page_header(ui: &mut Ui, view: &PrView, popped: bool, acts: &mut Vec<Action>) {
    let url = match &view.details {
        Some(Ok(details)) => Some(details.url.clone()),
        _ => None,
    };
    let title = pull_request_title(view.number);
    let loading = view.loading;
    widgets::panel_header(ui, &title, |ui| {
        let tip = popout::close_tip(
            popped,
            crate::i18n::text("repo-workflows-pr_view-page-header-80403162"),
        );
        if icons::small_button(ui, "x", tip).clicked() {
            acts.push(Action::ClosePr);
        }
        popout::button(ui, Panel::Pr, popped, acts);
        if let Some(url) = url
            && icons::small_button(
                ui,
                "external-link",
                crate::i18n::text("repo-workflows-pr_view-page-header-a7eac6b5"),
            )
            .clicked()
        {
            acts.push(Action::OpenUrl(url));
        }
        if loading {
            ui.add(crate::spinner::Spinner::new());
        } else if icons::small_button(
            ui,
            "refresh-cw",
            crate::i18n::text("repo-workflows-pr_view-page-header-d62bc4a3"),
        )
        .clicked()
        {
            acts.push(Action::ReloadPr);
        }
    });
}

/// What the page shows of a pull request that was read, top to bottom.
/// The sign-off state the page shows, and what fixing it needs: the branch checked out here
/// and whether the tab is free.
struct Dco<'a> {
    state: &'a dco::Dco,
    current: Option<&'a str>,
    idle: bool,
}

fn page(
    ui: &mut Ui,
    view: &mut PrView,
    draft: &mut PrDraft,
    width: f32,
    dco: &Dco,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let Some(Ok(details)) = &view.details else {
        return;
    };
    header(ui, details);
    section(
        ui,
        crate::i18n::text("repo-workflows-pr_view-page-9cc35840"),
    );
    if view.body.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-pr_view-page-06a69482"))
                .color(t.text_faint)
                .italics(),
        );
    }
    blocks(ui, &mut view.markdown, "body", &view.body, width);
    section(
        ui,
        crate::i18n::text("repo-workflows-pr_view-page-7503c27e"),
    );
    checks(ui, details, acts);
    if dco::pr_shows(dco.state, details) {
        section(
            ui,
            crate::i18n::text("repo-workflows-pr_view-page-0744b935"),
        );
        dco::pr_section(ui, dco.state, details, dco.current, dco.idle, acts);
    }
    if details.state == "OPEN" {
        section(
            ui,
            crate::i18n::text("repo-workflows-pr_view-page-fc2b737d"),
        );
        crate::repo::pr_merge::merge_box(ui, draft, details, acts);
    } else if let Some(error) = draft.error_at(Place::Merge) {
        // The box is gone, but what went wrong with it still counts.
        ui.add_space(8.0);
        ui.colored_label(t.warning, error);
    }
    section(
        ui,
        &crate::i18n::message(
            "repo-workflows-pr_view-page-607d89a5",
            &[(
                "details-timeline-len-1",
                (details.timeline.len()).clone().into(),
            )],
        ),
    );
    // Lines are shown in the changes of the commit the page names, not in those of an earlier
    // one still listed until the new ones come.
    let files = view
        .files
        .as_ref()
        .and_then(|f| f.as_ref().ok())
        .filter(|f| f.to == details.head_oid);
    timeline(
        ui,
        details,
        files,
        &view.entries,
        &mut view.markdown,
        draft,
        width,
        acts,
    );
    section(
        ui,
        crate::i18n::text("repo-workflows-pr_view-page-bc4bf274"),
    );
    pr_review::comment_box(ui, draft, acts);
    section(
        ui,
        &crate::i18n::message(
            "repo-workflows-pr_view-page-878ee775",
            &[(
                "details-commits-len-1",
                (details.commits.len()).clone().into(),
            )],
        ),
    );
    let signoffs = dco::pr_shows(dco.state, details)
        .then(|| dco.state.for_pr(details.number, &details.head_oid))
        .flatten();
    commits(ui, details, signoffs);
}

fn section(ui: &mut Ui, title: &str) {
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
fn header(ui: &mut Ui, d: &PrDetails) {
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
fn stats(ui: &mut Ui, d: &PrDetails) {
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

fn checks(ui: &mut Ui, d: &PrDetails, acts: &mut Vec<Action>) {
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
fn commits(ui: &mut Ui, d: &PrDetails, signoffs: Option<&dco::PrSignoffs>) {
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

fn pull_request_title(number: u64) -> String {
    crate::i18n::message(
        "repo-workflows-pr_view-page-header-38cc20ee",
        &[("view-number-1", number.to_string().into())],
    )
}
#[cfg(test)]
mod fluent_provider_identifier_tests {
    use super::*;
    #[test]
    fn fluent_partition_actual_pr_header_preserves_provider_identifier_while_counts_group() {
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            let title = pull_request_title(1234);
            assert!(title.ends_with("#1234"));
            assert!(!title.contains("1.234"));
            assert!(!title.contains("1,234"));
            assert_eq!(
                crate::i18n::number(1234usize),
                if language == "de" { "1.234" } else { "1,234" }
            );
        }
    }
}
