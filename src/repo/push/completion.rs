//! Push settlement, chained steps and remote rejection checks.

use super::*;

impl RepoTab {
    /// Takes in the end of an operation for Push and Sync (called last by `op_done`): Sync's
    /// push after its pull, and a look at the remote after a refused push.
    pub(in crate::repo) fn push_op_ended(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        if let Some(chain) = self.push_flow.then_push.take() {
            if chain.after == label {
                self.chain_step_ended(chain, result, ctx);
                return;
            }
            self.push_flow.then_push = Some(chain);
        }
        let Some(attempt) = self.push_flow.attempt.take_if(|a| a.label == label) else {
            return;
        };
        let branch_ref = format!("refs/heads/{}", attempt.branch);
        let refused = |out: &str| porcelain::refused(out, &branch_ref, attempt.commit.as_deref());
        match result {
            // Older git ends a push some of whose refs were refused with success all the same.
            Ok(out) if refused(out).is_some() => {
                let out = out.clone();
                self.status = Some(crate::repo::Notice::plain((
                    format!("{label} refused: {}", out.trim()),
                    true,
                )));
                self.push_failed(attempt, &out, ctx);
            }
            Ok(_) => {
                self.push_flow.banner = None;
                if let Some(done) = attempt.done {
                    self.status = Some(crate::repo::Notice::plain((done, false)));
                }
            }
            // Run again once signed in, and then taken in here again. Not a refusal whose
            // message (a hook's words) happens to say "403".
            Err(e) if refused(e).is_none() && git::is_auth_error(e) => {
                self.push_flow.attempt = Some(attempt)
            }
            Err(e) => self.push_failed(attempt, e, ctx),
        }
    }

    /// The operation a chain waited for ended.
    pub(in crate::repo::push) fn chain_step_ended(
        &mut self,
        mut chain: Chain,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        // After the pull, or after what ran after it (whatever came of that).
        if chain.pulled || result.is_ok() {
            // Something else started meanwhile (updating the submodules after the pull): the
            // push waits for it.
            if let Some(busy) = self.busy_op.clone() {
                chain.after = busy;
                chain.pulled = true;
                self.push_flow.then_push = Some(chain);
            } else {
                self.chained_push(chain.push, ctx);
            }
            return;
        }
        match result {
            // Pulled again once signed in.
            Err(e) if git::is_auth_error(e) => self.push_flow.then_push = Some(chain),
            Err(e) => {
                let what = chain.after;
                self.status = Some(crate::repo::Notice::plain((
                    format!("{what} failed, so nothing was pushed: {e}"),
                    true,
                )));
            }
            Ok(_) => {}
        }
    }

    /// A push that failed: refused by the remote (a banner then says why and what can be done),
    /// or failed otherwise (the status line already says how).
    pub(in crate::repo::push) fn push_failed(
        &mut self,
        attempt: Attempt,
        error: &str,
        ctx: &egui::Context,
    ) {
        let branch_ref = format!("refs/heads/{}", attempt.branch);
        let Some((line, why)) = porcelain::refused(error, &branch_ref, attempt.commit.as_deref())
        else {
            return;
        };
        match why {
            Refusal::Remote(_) => {
                self.push_flow.banner = Some(Banner::Refused {
                    branch: attempt.branch,
                    message: error.trim().to_owned(),
                });
            }
            Refusal::Behind | Refusal::Moved => {
                let Some(target) = refused_target(attempt.target, &line.to) else {
                    return;
                };
                let q = check::Question {
                    branch: attempt.branch,
                    target,
                    to: line.to,
                    remotes: self.remote_names(),
                    publish: attempt.publish,
                    moved: why == Refusal::Moved,
                };
                self.check_remote(q, ctx);
            }
            Refusal::Other(_) => {}
        }
    }

    /// Looks at the remote in the background (the tab busy meanwhile), then shows the banner.
    pub(in crate::repo::push) fn check_remote(&mut self, q: check::Question, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let what = q.target.as_ref().map_or_else(
            || crate::i18n::text("repo-workflows-push-check-remote-d6f3f0c2").to_owned(),
            Target::label,
        );
        self.busy_op = Some(format!("Checking {what}"));
        let path = self.path.clone();
        let creds = self.credentials.clone();
        // Let go of all the same if it panics.
        let failed = |e: String| -> Msg {
            Msg::Then(Box::new(move |tab: &mut RepoTab, _: &egui::Context| {
                tab.busy_op = None;
                tab.status = Some(crate::repo::Notice::plain((e, true)));
            }))
        };
        spawn_or(&self.tx, ctx, failed, move || {
            let checked = check::check(&path, &q, creds.as_ref());
            Msg::Then(Box::new(move |tab: &mut RepoTab, ctx: &egui::Context| {
                tab.remote_checked(checked, ctx)
            }))
        });
    }

    pub(in crate::repo::push) fn remote_checked(
        &mut self,
        checked: Result<Checked, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        match checked {
            Ok(Checked::Rejected(r)) => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("Push refused: {}", banner::headline(&r)),
                    true,
                )));
                self.push_flow.banner = Some(Banner::Rejected(r));
            }
            Ok(Checked::Gone {
                branch,
                target,
                publish,
            }) => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("Push refused: {} is gone", target.label()),
                    true,
                )));
                self.push_flow.banner = Some(Banner::Gone {
                    branch,
                    target,
                    publish,
                });
            }
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("Push refused, and the remote could not be checked: {e}"),
                    true,
                )))
            }
        }
        // The remote branch was fetched.
        self.reload(ctx);
    }
}
