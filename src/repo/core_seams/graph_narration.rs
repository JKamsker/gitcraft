use super::*;

impl RepoTab {
    pub(in crate::repo) fn update_graph_narration(&mut self, ctx: &egui::Context) {
        if !crate::settings::Settings::get(ctx).graph_lane_walk {
            self.graph_narration.reader.cancel();
            self.graph_narration.hash.clear();
            self.graph_narration.result = None;
            return;
        }
        if let Some(result) = self.graph_narration.reader.poll(&self.tx, ctx) {
            self.graph_narration.result = Some(result.unwrap_or_else(|e| e));
        }
        if self.topology.parents.len() != self.commits.len() {
            return;
        }
        let Some(selected) = self.selected else {
            return;
        };
        let Some(commit) = self.commits.get(selected) else {
            return;
        };
        if commit.hash == self.graph_narration.hash {
            return;
        }
        self.graph_narration.hash = commit.hash.clone();
        self.graph_narration.result = None;
        self.graph_narration.reader.cancel();
        if self.is_wip(selected) || self.stash_at(selected).is_some() {
            return;
        }
        let Some(baseline) = self
            .refs
            .local
            .iter()
            .find(|b| matches!(b.name.as_str(), "main" | "master" | "trunk"))
            .or_else(|| self.refs.local.iter().find(|b| b.is_head))
        else {
            return;
        };
        let Some(&base) = self.index.get(&baseline.target) else {
            return;
        };
        let name = baseline.name.clone();
        let topology = self.topology.clone();
        let commits = self.commits.clone();
        let releases: Vec<_> = self
            .refs
            .local
            .iter()
            .filter(|b| b.name.starts_with("release/"))
            .filter_map(|b| self.index.get(&b.target).map(|row| (b.name.clone(), *row)))
            .take(20)
            .collect();
        self.graph_narration.reader.queue(move |active| {
            let selected_set = topology.ancestors(selected, active);
            let base_set = topology.ancestors(base, active);
            if !active() {
                return Err(crate::i18n::text("repo-core-2747").into());
            }
            let ahead = selected_set.difference(&base_set).count();
            let behind = base_set.difference(&selected_set).count();
            let common = selected_set.intersection(&base_set).copied().min();
            let complete = selected_set.union(&base_set).all(|row| {
                commits.get(*row).is_some_and(|c| {
                    c.parents.len() == topology.parents.get(*row).map_or(0, Vec::len)
                })
            });
            let mut result = crate::i18n::message(
                "repo-core-2758",
                &[
                    ("ahead", (ahead).into()),
                    ("behind", (behind).into()),
                    ("name", name.to_string().into()),
                    ("count", (commits.len()).into()),
                ],
            );
            if let Some(common) = common.and_then(|row| commits.get(row)) {
                result.push_str(&crate::i18n::message(
                    "repo-core-2759",
                    &[
                        (
                            "hash",
                            common
                                .hash
                                .get(..8)
                                .unwrap_or(&common.hash)
                                .to_string()
                                .into(),
                        ),
                        ("date", common.date.to_string().into()),
                    ],
                ));
            } else {
                result.push_str(crate::i18n::text("repo-core-2760"));
            }
            let mut descendants = HashSet::new();
            let mut todo = vec![selected];
            while let Some(row) = todo.pop() {
                if !active() {
                    return Err(crate::i18n::text("repo-core-2747").into());
                }
                if descendants.insert(row) {
                    todo.extend(topology.children.get(row).into_iter().flatten());
                }
            }
            for (release, tip) in releases {
                let (merges, line_complete) =
                    topology.release_integrations(tip, &descendants, &commits, active)?;
                result.push_str(&crate::i18n::message(
                    "repo-core-2761",
                    &[
                        ("merges", format!("{}", merges).into()),
                        ("release", release.to_string().into()),
                    ],
                ));
                if !line_complete {
                    result.push_str(crate::i18n::text("repo-core-2762"));
                }
            }
            if !complete {
                result.push_str(crate::i18n::text("repo-core-2763"));
            }
            if !active() {
                return Err(crate::i18n::text("repo-core-2747").into());
            }
            Ok(result)
        });
    }

    /// Selected-commit comparison is computed on a worker with pinned immutable topology.
    /// The header reports only local observations; it never implies unloaded ancestry is absent.
    pub(in crate::repo) fn graph_walk_header(&mut self, ui: &mut egui::Ui) {
        let mut settings = crate::settings::Settings::get(ui.ctx());
        self.update_graph_narration(ui.ctx());
        if !settings.graph_lane_walk {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(
                    &mut settings.graph_lane_walk,
                    crate::i18n::text("repo-core-0222"),
                )
                .changed()
            {
                settings.set(ui.ctx());
            }
            if settings.graph_lane_walk {
                ui.small(crate::i18n::message(
                    "repo-core-2764",
                    &[
                        (
                            "GraphNext",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphNext)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphPrevious",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphPrevious)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphRight",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphRight)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphLeft",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphLeft)
                                .to_string()
                                .into(),
                        ),
                    ],
                ));
                if let Some(row) = self.selected {
                    let relations: Vec<_> = self
                        .topology
                        .parents
                        .get(row)
                        .into_iter()
                        .flatten()
                        .map(|r| (crate::i18n::text("repo-core-0882"), *r))
                        .chain(
                            self.topology
                                .children
                                .get(row)
                                .into_iter()
                                .flatten()
                                .map(|r| (crate::i18n::text("repo-core-2765"), *r)),
                        )
                        .collect();
                    let mut chosen = None;
                    ui.menu_button(crate::i18n::text("repo-core-2766"), |ui| {
                        for (kind, row) in relations {
                            if let Some(c) = self.commits.get(row)
                                && ui
                                    .button(crate::i18n::message(
                                        "repo-core-2767",
                                        &[
                                            ("kind", kind.to_string().into()),
                                            (
                                                "hash",
                                                c.hash
                                                    .get(..8)
                                                    .unwrap_or(&c.hash)
                                                    .to_string()
                                                    .into(),
                                            ),
                                            ("summary", c.summary.to_string().into()),
                                        ],
                                    ))
                                    .clicked()
                            {
                                chosen = Some(row);
                                ui.close();
                            }
                        }
                    });
                    if let Some(row) = chosen {
                        self.select(row, ui.ctx());
                        self.scroll_to = Some(row);
                    }
                }
                if let Some(row) = self.selected {
                    ui.small(self.graph_description(row));
                }
                if self
                    .selected
                    .and_then(|row| self.commits.get(row))
                    .is_some_and(|c| c.hash == self.graph_narration.hash)
                    && let Some(description) = &self.graph_narration.result
                {
                    ui.small(description);
                }
                if ui
                    .checkbox(
                        &mut settings.graph_sounds,
                        crate::i18n::text("repo-core-2768"),
                    )
                    .changed()
                {
                    settings.set(ui.ctx());
                }
                if crate::audio::unavailable() {
                    ui.small(crate::i18n::text("repo-core-2769"));
                    if ui.button(crate::i18n::text("repo-core-2770")).clicked() {
                        crate::audio::retry();
                    }
                }
            }
        });
    }
}
