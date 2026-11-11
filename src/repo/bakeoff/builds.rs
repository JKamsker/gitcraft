use super::*;

impl RepoTab {
    pub(in crate::repo) fn build(&mut self, n: usize, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let command = view.build_command.trim().to_owned();
        let Some(Ok(comparison)) = &view.comparison else {
            return;
        };
        let Some(compared) = comparison.attempts.get(n) else {
            return;
        };
        if view.builds.get(n) == Some(&Some(Built::Preparing)) {
            return;
        }
        let tree = compared.tree.clone();
        let base = comparison.base.clone();
        let request = view.request;
        if command.is_empty() {
            view.remember_note(BakeNote::CommandMissing, true);
            return;
        }
        let name = view
            .attempts
            .get(n)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        if let Some(slot) = view.builds.get_mut(n) {
            *slot = Some(Built::Preparing);
        }
        if let Some(status) = view.statuses.get_mut(n) {
            *status = None;
        }
        if let Some(launch) = view.launches.get_mut(n) {
            *launch = None;
        }
        let repo = self.path.clone();
        let remembered = command.clone();
        self.then(
            ctx,
            move || -> Result<(PathBuf, PathBuf), String> {
                let _ = crate::agent::write_json(&repo, build::COMMAND_FILE, &remembered);
                let worktree = build::workspace(&repo, &tree, &base)?;
                let status = build::status_file(&repo, &worktree)?;
                // An earlier build's status isn't this one's.
                if let Some(dir) = status.parent() {
                    let _ = crate::remote::create_dir_all(dir);
                }
                crate::remote::write(&status, b"").map_err(|e| {
                    crate::i18n::message(
                        "repo-workflows-bakeoff-build-06d160c3",
                        &[("e", (e).to_string().into())],
                    )
                })?;
                Ok((worktree, status))
            },
            move |tab, status, ctx| match status {
                Ok((worktree, status)) => {
                    if tab
                        .bakeoff
                        .view
                        .as_ref()
                        .is_none_or(|v| v.request != request)
                    {
                        return;
                    }
                    let there = build::for_sh(&status);
                    if let Some(view) = &mut tab.bakeoff.view
                        && let Some(slot) = view.statuses.get_mut(n)
                    {
                        *slot = Some(status.clone());
                    }
                    let launched = tab.start_launch_tracked(
                        Launch {
                            label: format!("build · {name}"),
                            worktree,
                            run: Some(build::script(&command, &there)),
                            ..Launch::default()
                        },
                        ctx,
                    );
                    if let Some(view) = &mut tab.bakeoff.view {
                        view.accept_launch(request, n, &status, launched);
                    }
                }
                Err(e) => {
                    if let Some(view) = &mut tab.bakeoff.view
                        && view.request == request
                    {
                        view.note_presentation = None;
                        view.note = Some((e, true));
                        if let Some(slot) = view.builds.get_mut(n) {
                            *slot = Some(Built::Unavailable);
                        }
                    }
                }
            },
        );
    }

    /// Every frame the bake-off is shown: the builds still running are looked at now and then.
    pub(in crate::repo) fn poll_builds(&mut self, ctx: &egui::Context) {
        // Atomic observation only: no try_wait, process search or signals on UI.
        let terminals = self
            .terminals
            .shells()
            .iter()
            .map(|terminal| terminal.output_lifetime())
            .collect::<Vec<_>>();
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let running: Vec<(usize, PathBuf, bool)> = view
            .builds
            .iter()
            .zip(&view.statuses)
            .enumerate()
            .filter(|(_, (b, _))| **b == Some(Built::Running))
            .filter_map(|(i, (_, file))| {
                file.clone().map(|f| {
                    let open = view
                        .launches
                        .get(i)
                        .and_then(|launch| launch.as_ref())
                        .is_some_and(|witness| {
                            !witness.load(std::sync::atomic::Ordering::Relaxed)
                                && terminals
                                    .iter()
                                    .any(|terminal| std::sync::Arc::ptr_eq(terminal, witness))
                        });
                    (i, f, open)
                })
            })
            .collect();
        if running.is_empty() || view.polled.is_some_and(|t| t.elapsed() < BUILD_POLL) {
            if !running.is_empty() {
                ctx.request_repaint_after(BUILD_POLL);
            }
            return;
        }
        view.polled = Some(Instant::now());
        let request = view.request;
        self.then(
            ctx,
            move || {
                running
                    .into_iter()
                    .map(|(i, file, open)| {
                        let text = crate::remote::read_to_string(&file).ok();
                        (i, file, build::observed_status(text.as_deref(), open))
                    })
                    .collect::<Vec<_>>()
            },
            move |tab, found, _| {
                if let Some(view) = &mut tab.bakeoff.view {
                    view.accept_builds(request, found);
                }
            },
        );
    }
}
