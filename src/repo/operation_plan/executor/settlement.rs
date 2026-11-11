//! Bind inverse plans only to refs, rewrites and pushes actually owned by this execution.
use super::*;

impl Execution<'_> {
    pub(super) fn record_rewrite(
        &mut self,
        before: Option<(String, String)>,
        result: &Result<String, String>,
    ) {
        let Some((branch, before)) = before else {
            return;
        };
        let done = Rewrite {
            tip: git::resolve(self.path, &branch).unwrap_or_default(),
            branch,
            before,
            conflicts: result.as_ref().err().is_some_and(|error| {
                error.contains("CONFLICT") || error.contains("could not apply")
            }),
        };
        if let Some(scope) = self.fork_scope {
            scope.record_rewrite(done.clone());
        }
        if !done.tip.is_empty() {
            self.owned_refs
                .push((done.branch.clone(), done.tip.clone()));
        }
        self.rewrite = Some(done);
    }

    pub(super) fn record_success(
        &mut self,
        op: &[String],
        reset: Option<(String, String)>,
    ) -> Result<bool, StepFailure> {
        if let Some(done) = reset {
            self.owned_refs.push(done);
        }
        if self.checkout.is_some()
            && op.first().is_some_and(|word| word == "checkout")
            && let Ok(branch) = git::run(Some(self.path), &["symbolic-ref", "-q", "HEAD"])
        {
            self.owned_refs
                .push((branch.trim().to_owned(), git::head(self.path)));
        }
        // Pin the just-created remote before config/fetch/checkout. Retrying this
        // post-command failure must not repeat a successful remote-add command.
        if matches!(op, [remote, add, ..] if remote == "remote" && add == "add") {
            self.fork_scope
                .map_or(Ok(()), |scope| scope.check_remote(self.path))
                .and_then(|()| {
                    self.checkout
                        .as_mut()
                        .map_or(Ok(()), |checkout| checkout.check(self.path))
                })
                .map_err(|error| StepFailure {
                    error,
                    completed: true,
                })?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn finish(&self, kind: OpKind, success: bool) -> OpKind {
        // A successful no-op stash must never authorize popping an older stash.
        let no_stash = self
            .stash_before
            .as_ref()
            .is_some_and(|before| *before == git::resolve(self.path, "refs/stash").ok());
        match kind {
            OpKind::Normal(Undo::HeadMove { mode }, before) if success => {
                let before = self
                    .head_before
                    .as_ref()
                    .filter(|head| !head.is_empty())
                    .unwrap_or(&before);
                let branch =
                    git::run_ok_codes(Some(self.path), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
                        .map(|value| value.trim().to_owned())
                        .unwrap_or_default();
                OpKind::Normal(
                    Undo::HeadMove { mode },
                    format!("{before}\u{0}{}\u{0}{branch}", git::head(self.path)),
                )
            }
            OpKind::Normal(Undo::Custom { .. }, before) if no_stash => {
                OpKind::Normal(Undo::None, before)
            }
            OpKind::Normal(Undo::Custom { mut undo, mut redo }, before) if success => {
                self.bind_inverse(&mut undo, &mut redo);
                OpKind::Normal(Undo::Custom { undo, redo }, before)
            }
            OpKind::Undo(mut entry) if success => {
                self.bind_inverse(&mut entry.redo, &mut entry.undo);
                OpKind::Undo(entry)
            }
            OpKind::Redo(mut entry) if success && !no_stash => {
                self.bind_inverse(&mut entry.undo, &mut entry.redo);
                OpKind::Redo(entry)
            }
            other => other,
        }
    }

    fn bind_inverse(&self, inverse: &mut [Vec<String>], original: &mut [Vec<String>]) {
        fill_expected(self.path, inverse, &self.owned_refs);
        if let Some(checkout) = &self.checkout {
            checkout.bind(inverse);
            checkout.bind(original);
        }
        crate::repo::forks::guard::bind_leases(inverse, &self.pushed);
    }
}
