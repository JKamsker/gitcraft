//! Pull-request row rendering and contextual actions.

use super::*;

impl RepoTab {
    pub(super) fn pr_row(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        forge: Forge,
        pr: &PullRequest,
        idle: bool,
    ) {
        let t = Tokens::get(ui.ctx());
        // Known by its number, not its place: the list reloads (and reorders) on its own, and a
        // menu left open (Check out) must stay this one's.
        let (rect, r) = widgets::keyed_row(ui, 36.0, ("pr-row", pr.number));
        widgets::a11y(&r, egui::WidgetType::Button, || {
            labels::pull_request(pr.number, &pr.title, &pr.author, &pr.branch, pr.draft)
        });
        let shown = self.pr_view.as_ref().is_some_and(|v| v.number == pr.number)
            || self.compare.as_ref().is_some_and(|c| {
                c.target_label == pr.branch && c.target.as_deref() == Some(pr.sha.as_str())
            });
        if shown {
            ui.painter().rect_filled(rect, 0.0, t.row_selected);
        } else if r.hovered() {
            ui.painter().rect_filled(rect, 0.0, t.hover);
        }
        let y = rect.center().y;
        icons::paint(
            ui,
            Rect::from_center_size(pos2(rect.left() + 34.0, y), vec2(16.0, 16.0)),
            "git-pull-request",
            13.0,
            if pr.draft { t.text_faint } else { t.success },
        );
        let ci = self.ci_status(&pr.sha);
        // The build status on the right, and the texts up to it.
        let text_right = if ci.is_some() {
            rect.right() - 24.0
        } else {
            rect.right()
        };
        let p = ui.painter_at(rect).with_clip_rect(Rect::from_min_max(
            rect.min,
            pos2(text_right, rect.bottom()),
        ));
        p.text(
            pos2(rect.left() + 48.0, y - 7.0),
            Align2::LEFT_CENTER,
            format!("#{} {}", pr.number, pr.title),
            egui::FontId::proportional(12.5),
            t.text,
        );
        let moved = self.pr_moved_since_seen(pr);
        // First, where a long branch name can't cut it off.
        let sub = format!(
            "{}{} · {}{}",
            if moved {
                crate::i18n::text("repo-core-3449")
            } else {
                ""
            },
            pr.branch,
            pr.author,
            if pr.draft {
                crate::i18n::text("repo-core-3450")
            } else {
                ""
            }
        );
        let sub = p.text(
            pos2(rect.left() + 48.0, y + 8.0),
            Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
        let badges = crate::repo::forks::paint_badges(
            self,
            &p,
            pos2(sub.right() + 8.0, y + 8.0),
            pr.number,
            &pr.sha,
        );
        if let Some(status) = ci {
            let center = pos2(rect.right() - 14.0, y);
            acts.extend(ci_status::dot(ui, r.id.with("ci"), center, &pr.sha, status));
        }
        let has_view = forge.has_pages();
        // Without a page here, opening it in the browser is looking at it.
        // Not when it changed since: that would take away what changed before it is compared.
        let seen_on_open = (!has_view && !moved).then_some(Action::Versions(
            crate::repo::versions::VersionsAct::PrSeen(pr.number),
        ));
        let r = r.on_hover_text(if has_view {
            crate::i18n::message("repo-core-3451", &[("url", pr.url.to_string().into())])
        } else {
            crate::i18n::message("repo-core-3452", &[("url", pr.url.to_string().into())])
        });
        let r = match badges {
            Some(why) => r.on_hover_text(why),
            None => r,
        };
        if r.double_clicked() {
            if has_view {
                acts.push(Action::OpenPr(pr.number));
            } else {
                acts.push(Action::OpenUrl(pr.url.clone()));
                acts.extend(seen_on_open);
            }
        } else if r.clicked() {
            acts.push(Action::SelectPr(pr.number));
        }
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| {
                if has_view
                    && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3453"))
                        .icon("eye")
                        .show(ui)
                        .clicked()
                {
                    acts.push(Action::OpenPr(pr.number));
                }
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3454"))
                    .icon("external-link")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                    if !has_view && !moved {
                        acts.push(Action::Versions(
                            crate::repo::versions::VersionsAct::PrSeen(pr.number),
                        ));
                    }
                }
                self.pr_since_items(ui, acts, pr);
                self.pr_worktree_button(ui, acts, pr.number, &pr.branch, &pr.sha, idle, true);
                if crate::menu_item::MenuItem::new(crate::i18n::message(
                    "repo-core-3455",
                    &[("branch", pr.branch.to_string().into())],
                ))
                .icon("log-in")
                .enabled(idle)
                .show(ui)
                .clicked()
                {
                    if self.refs.local.iter().any(|b| b.name == pr.branch) {
                        acts.push(Action::Checkout(pr.branch.clone()));
                    } else if let Some(b) = self.pr_remote_branch(pr) {
                        acts.push(Action::CheckoutRemote(b.name.clone()));
                    } else {
                        let Some(remote) =
                            self.primary_forge_remote().map(str::to_owned).or_else(|| {
                                self.hosting
                                    .is_none()
                                    .then(|| self.default_remote())
                                    .flatten()
                            })
                        else {
                            return;
                        };
                        acts.push(Action::CheckoutRemote(format!("{remote}/{}", pr.branch)));
                    }
                }
                self.fork_items(ui, acts, pr.number, idle, false);
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3456"))
                    .icon("copy")
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(pr.url.clone());
                }
            });
    }
}
