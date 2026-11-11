use super::*;

impl RepoTab {
    pub(crate) fn resolver_act(&mut self, act: ResolverAct, ctx: &egui::Context) {
        match act {
            ResolverAct::Start(files) => self.start_resolver(files, ctx),
            ResolverAct::Stop => {
                if let Some(run) = &mut self.resolver.run {
                    run.stopping = true;
                    if let Some(agent) = &run.agent {
                        agent.stop();
                    }
                }
            }
            // A merge put aside is brought back first: there is nothing to apply it to.
            ResolverAct::Apply
                if self
                    .resolver
                    .run
                    .as_ref()
                    .is_some_and(|r| r.parked.is_some()) => {}
            ResolverAct::Apply => self.apply_resolved(ctx),
            ResolverAct::BringBack(id) => self.bring_back(&id, ctx),
            ResolverAct::DropParked(id) => self.drop_parked(&id, ctx),
            ResolverAct::Pick(path, on) => {
                if let Some(run) = &mut self.resolver.run {
                    if on {
                        run.picked.insert(path);
                    } else {
                        run.picked.remove(&path);
                    }
                }
            }
            ResolverAct::Dismiss => {
                // While files are written, the run stays to mark them resolved; a merge put
                // aside is never forgotten with it: it is brought back first.
                if self
                    .resolver
                    .run
                    .as_ref()
                    .is_some_and(|r| r.phase == Phase::Applying || r.parked.is_some())
                {
                    return;
                }
                if let Some(mut run) = self.resolver.run.take() {
                    clean_up(&mut run);
                }
                self.resolver.folded = false;
            }
            ResolverAct::Fold(folded) => self.resolver.folded = folded,
            ResolverAct::Review(i) => self.open_review(i, ctx),
            ResolverAct::ReviewAsDiff(on) => {
                if let Some(r) = self.resolver.run.as_mut().and_then(|r| r.review.as_mut()) {
                    r.as_diff = on;
                }
            }
            ResolverAct::CloseReview => {
                if let Some(run) = &mut self.resolver.run {
                    run.review = None;
                }
            }
        }
    }

    /// Shows the agent's work on its files, at the `i`th: read in the background the first
    /// time.
    pub(in crate::repo) fn open_review(&mut self, i: usize, ctx: &egui::Context) {
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Ready)
        else {
            return;
        };
        if let Some(review) = &mut run.review {
            review.selected = i;
            return;
        }
        let Some(ws) = run.workspace.clone() else {
            return;
        };
        run.review = Some(ReviewState {
            files: None,
            selected: i,
            as_diff: false,
        });
        let paths: Vec<String> = run.results.iter().map(|(p, _)| p.clone()).collect();
        let id = run.id.clone();
        self.then(
            ctx,
            move || {
                paths
                    .into_iter()
                    .map(|p| {
                        let r = review::of_file(&ws, &p);
                        (p, r)
                    })
                    .collect::<Vec<_>>()
            },
            move |tab, files, _| {
                let review = tab
                    .resolver
                    .run
                    .as_mut()
                    .filter(|r| r.id == id)
                    .and_then(|r| r.review.as_mut());
                if let Some(review) = review {
                    review.files = Some(files);
                }
            },
        );
    }

    /// Selects and shows the row the tab was waiting to (see `ResolverState::reveal`), once
    /// the graph read after the change has it.
    pub(in crate::repo) fn resolver_reveal(&mut self, ctx: &egui::Context) {
        let Some((want, since)) = self.resolver.reveal.clone() else {
            return;
        };
        if since.elapsed() > REVEAL_WITHIN {
            self.resolver.reveal = None;
            return;
        }
        if self.loading {
            return;
        }
        let row = match &want {
            Reveal::Parked(id) => graph::parked_row_of(&self.commits, id),
            Reveal::Changes => self
                .commits
                .iter()
                .position(|c| c.hash == crate::repo::WIP_HASH),
        };
        if let Some(row) = row {
            self.resolver.reveal = None;
            self.select(row, ctx);
            self.scroll_to = Some(row);
        }
    }

    // Whether the run `id` is this tab's.
}
