use super::conflict_labels::SETTLE;
use super::*;

impl RepoTab {
    /// Reads the repository again and forgets the conflict checks (F5, the toolbar's button,
    /// coming back to the window): `.gitattributes` or git's config may have changed.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.invalidate_repository_context();
        self.conflict_checks.forget();
        self.reload(ctx);
    }

    /// Whether a read is going or began a moment ago: the window gaining the focus then (as it
    /// does just after starting) reads nothing it won't have.
    pub fn just_read(&self) -> bool {
        self.loading
            || self
                .last_load_started
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(3))
    }

    /// A menu entry for `op` with `other`, marked with whether it would conflict.
    pub(in crate::repo) fn conflict_button(
        &self,
        ui: &mut Ui,
        text: String,
        enabled: bool,
        op: Op,
        other: &str,
    ) -> Response {
        let icon = match op {
            Op::Merge | Op::Rebase => "git-merge",
            Op::CherryPick => "git-branch",
        };
        let refused = (!enabled).then_some(if other == self.refs.head {
            crate::i18n::text("repo-core-0796")
        } else {
            crate::i18n::text("repo-core-0797")
        });
        if !enabled || self.refs.head.is_empty() || other == self.refs.head {
            return crate::menu_item::MenuItem::new(text)
                .icon(icon)
                .refused(refused)
                .show(ui);
        }
        let key = Key::new(op, &self.refs.head, other);
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        // The entry and its mark on one line: the menu widens for them.
        let mut button = crate::menu_item::MenuItem::new(text).icon(icon);
        if let Some((words, tone)) = label(op, &check) {
            let t = Tokens::get(ui.ctx());
            let color = match tone {
                Tone::Faint => t.text_faint,
                Tone::Good => t.success,
                Tone::Warn => t.warning,
            };
            button = button.annotation(RichText::new(words).color(color).small());
        }
        button
            .show(ui)
            .on_hover_text(tooltip(op, &check, !self.work.is_clean()))
    }

    /// The branch the checked-out one is compared with for the toolbar's badge, and its
    /// commit: its upstream, or the default branch when it has none (or it is gone).
    fn conflict_target(&self) -> Option<(String, String, bool)> {
        let current = self.refs.current.as_deref()?;
        let branch = self.refs.local.iter().find(|b| b.name == current)?;
        let local = || self.refs.local.iter();
        let remote = || self.refs.remotes.iter().flat_map(|(_, list)| list);
        if !branch.upstream.is_empty() && !branch.upstream_gone() {
            // `%(upstream:short)` names a remote branch as `origin/x`, a local one as `x`.
            let name = &branch.upstream;
            let found = remote()
                .find(|b| b.name == *name)
                .or_else(|| local().find(|b| b.name == *name))?;
            return Some((name.clone(), found.target.clone(), true));
        }
        let base = self.base_ref()?;
        if !base.remote && base.name == current {
            return None;
        }
        let found = if base.remote {
            remote().find(|b| b.name == base.name)
        } else {
            local().find(|b| b.name == base.name)
        }?;
        Some((base.name, found.target.clone(), false))
    }

    /// The toolbar's badge, when merging the checked-out branch's target into it would
    /// conflict. Checked once the refs have settled, and not while an operation runs.
    pub(in crate::repo) fn target_conflict_badge(&mut self, ui: &mut Ui) {
        let target = self.conflict_target();
        // With Pull set to rebase, the upstream is rebased onto, not merged: a branch can
        // merge cleanly and still stop at one of its commits.
        let rebase = crate::settings::Settings::get(ui.ctx()).pull_mode
            == crate::repo::pull::PullMode::Rebase;
        let key = target
            .as_ref()
            .filter(|(_, commit, _)| !self.refs.head.is_empty() && *commit != self.refs.head)
            .map(|(name, commit, upstream)| {
                if rebase && *upstream {
                    Key::pull_rebase(&self.refs.head, commit, name)
                } else {
                    Key::new(Op::Merge, &self.refs.head, commit)
                }
            });
        let watch = &mut self.target_watch;
        // Named as it is now, even when another branch at the same commit took its place.
        watch.upstream = target.as_ref().is_some_and(|(_, _, upstream)| *upstream);
        watch.target = target.map(|(name, _, _)| name).unwrap_or_default();
        if watch.key != key {
            watch.key = key;
            watch.since = Some(Instant::now());
        }
        let Some(key) = watch.key.clone() else {
            return;
        };
        let settled = watch.since.map_or(Duration::ZERO, |s| s.elapsed());
        if settled < SETTLE {
            ui.ctx().request_repaint_after(SETTLE - settled);
            return;
        }
        if self.busy_op.is_some() || self.work.in_progress.is_some() {
            return;
        }
        let op = key.op;
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        let Check::Done(verdict) = &check else {
            return;
        };
        let what = match &verdict.prediction.outcome {
            Outcome::Conflicts { .. } => crate::i18n::text("repo-core-0798"),
            Outcome::Unsure(_) => crate::i18n::text("repo-core-0799"),
            Outcome::Clean { .. } => return,
        };
        let t = Tokens::get(ui.ctx());
        let target = self.target_watch.target.clone();
        let current = self.refs.current.clone().unwrap_or_default();
        ui.add_space(6.0);
        let why = if self.target_watch.upstream {
            crate::i18n::text("repo-core-0800")
        } else {
            crate::i18n::text("repo-core-0801")
        };
        let doing = match op {
            Op::Rebase => crate::i18n::message(
                "repo-core-0802",
                &[
                    ("current", current.to_string().into()),
                    ("target", target.to_string().into()),
                    ("why", why.to_string().into()),
                ],
            ),
            _ => crate::i18n::message(
                "repo-core-0803",
                &[
                    ("target", target.to_string().into()),
                    ("why", why.to_string().into()),
                    ("current", current.to_string().into()),
                ],
            ),
        };
        crate::repo::widgets::chip(ui, &format!("{what} {target}"), t.warning).on_hover_text(
            format!("{doing}:\n{}", tooltip(op, &check, !self.work.is_clean())),
        );
    }
}
