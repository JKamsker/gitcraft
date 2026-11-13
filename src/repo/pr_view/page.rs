//! The page of the pull request in the big area, and the files it changes in the right pane.

mod sections;
use sections::{checks, commits, header, section};

use egui::{Align2, RichText, Sense, Ui, pos2, vec2};

use crate::pull_request::{self as pr, CheckState, PrDetails};
use crate::repo::popout::{self, Panel};
use crate::repo::pr_review::{self, Place, PrDraft};
use crate::repo::{Action, DiffKey, RepoTab, dco, short, widgets};
use crate::theme::{self, Tokens};
use crate::{git, icons};

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
                &[("relocated-len-1", (relocated.len()).into())],
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
                                    ("target-line-2", (target.line).into()),
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
                                    (e.lines().next().unwrap_or_default()).into(),
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
                if self.purpose_files(ui, acts, files) {
                    return;
                }
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
            &[("details-timeline-len-1", (details.timeline.len()).into())],
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
            &[("details-commits-len-1", (details.commits.len()).into())],
        ),
    );
    let signoffs = dco::pr_shows(dco.state, details)
        .then(|| dco.state.for_pr(details.number, &details.head_oid))
        .flatten();
    commits(ui, details, signoffs);
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
