//! Guard and prepare each external command, then settle its exact dispatch result.
use super::*;

struct Command {
    actual: Vec<String>,
    pushed: Option<(Vec<String>, Pushed)>,
    rewrite_before: Option<(String, String)>,
    reset_result: Option<(String, String)>,
}

impl Execution<'_> {
    pub(super) fn step(
        &mut self,
        step: &model::PlannedStep<'_>,
    ) -> Result<Option<String>, StepFailure> {
        let op = step.argv;
        self.check_guards()?;
        if self.check_expectation(&step.kind)? {
            return Ok(None);
        }
        crate::repo::partial_stash::validate_push(self.path, op)?;
        if let model::Step::Try { note, args } = &step.kind {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Err(error) = git::run_unattended(Some(self.path), &args, self.creds) {
                self.tried.push(format!("{note} ({})", headline(&error)));
            }
            return Ok(None);
        }
        let command = self.prepare_command(op)?;
        let mut result = self.dispatch(step, &command.actual);
        self.record_rewrite(command.rewrite_before, &result);
        if result.is_ok() && self.record_success(op, command.reset_result)? {
            result = Ok(String::new());
        }
        crate::repo::forks::guard::completed_push(&result, command.pushed, &mut self.pushed);
        result.map(Some).map_err(Into::into)
    }

    fn check_guards(&mut self) -> Result<(), String> {
        self.fork_scope
            .map_or(Ok(()), |scope| scope.check_remote(self.path))?;
        crate::repo::forks::guard::check(self.path, self.ops)?;
        if let Some(checkout) = &mut self.checkout {
            checkout.check(self.path)?;
        }
        self.fork_scope.map_or(Ok(()), Scope::check)
    }

    fn check_expectation(&self, kind: &model::Step<'_>) -> Result<bool, String> {
        match kind {
            model::Step::Expectation(model::Precondition::FoldSafe(target)) => {
                git::fold_read(self.path, target)
                    .and_then(crate::repo::fold::plan::Plan::from_read)?;
            }
            model::Step::Expectation(model::Precondition::FoldIgnored(target)) => {
                git::fold_read(self.path, target)
                    .and_then(crate::repo::fold::plan::Plan::check_ignored)?;
            }
            _ => {}
        }
        Ok(matches!(kind, model::Step::Expectation(_)))
    }

    fn prepare_command(&mut self, op: &[String]) -> Result<Command, String> {
        let reviewed = reviewed_push_args(op)?;
        let capture_args = reviewed.as_deref().unwrap_or(op);
        let pushed = crate::repo::forks::guard::prepare_push(
            self.path,
            capture_args,
            self.guarded_fork,
            self.rewrite.as_ref(),
        )?;
        // Reviewed evidence must bind the same argv that the guarded sender uses.
        if reviewed.is_some()
            && pushed
                .as_ref()
                .is_some_and(|(args, _)| args != capture_args)
        {
            return Err("The reviewed fork source changed; reopen push preflight".into());
        }
        let mut actual = pushed
            .as_ref()
            .map_or_else(|| op.to_vec(), |(args, _)| args.clone());
        if actual.first().is_some_and(|arg| arg == "pull") {
            actual = ignored_safe_pull(self.path, &actual)?;
        }
        self.check_submodule(&actual)?;
        let rewrite_before = self.prepare_rewrite(op)?;
        let reset_result = self.prepare_local_mutation(op, &mut actual)?;
        if let Some(folder) = crate::agent::added_worktree(op) {
            if self.control.is_some_and(|control| control.is_cancelled()) {
                return Err(git::CANCELLED.to_owned());
            }
            git::prepare_worktree_folder(self.path, folder)?;
        }
        Ok(Command {
            actual,
            pushed,
            rewrite_before,
            reset_result,
        })
    }

    fn check_submodule(&self, actual: &[String]) -> Result<(), String> {
        if command_envelope(actual).is_ok_and(|(at, command)| {
            command == "submodule" && actual.get(at + 1).is_some_and(|arg| arg == "update")
        }) {
            if actual.first().is_some_and(|arg| arg != "submodule") {
                return Err("Submodule updates with global options cannot be validated safely. Run Update submodules again.".into());
            }
            check_ignored_submodule_update_with(actual, |args, codes| {
                git::run_ok_codes(Some(self.path), args, codes)
            })?;
        }
        Ok(())
    }

    fn prepare_rewrite(&self, op: &[String]) -> Result<Option<(String, String)>, String> {
        let rebasing = (self.fork_scope.is_some() || self.guarded_fork)
            && matches!(op, [c, config, rebase, _] if c == "-c" && config == "rebase.updateRefs=false" && rebase == "rebase");
        if !rebasing {
            return Ok(None);
        }
        // A rejected plan must never acquire rewrite ownership.
        check_expected(self.path, self.ops, self.control)?;
        Ok(git::run(Some(self.path), &["symbolic-ref", "-q", "HEAD"])
            .ok()
            .map(|branch| (branch.trim().to_owned(), git::head(self.path))))
    }

    fn prepare_local_mutation(
        &self,
        op: &[String],
        actual: &mut [String],
    ) -> Result<Option<(String, String)>, String> {
        let resetting = matches!(op, [reset, mode, _] if reset == "reset" && mode == "--keep");
        let deleting = matches!(op, [branch, delete, ..] if branch == "branch" && delete == "-D");
        if self.guarded_fork && (resetting || deleting) {
            check_local_expected(self.path, self.ops, op, self.control)?;
        }
        if resetting
            && self.ops.iter().any(|step| {
                expected_ref(step).is_some_and(|(name, _)| name.starts_with("refs/heads/"))
            })
            && let Ok(branch) = git::run(Some(self.path), &["symbolic-ref", "-q", "HEAD"])
        {
            let tip = git::resolve(self.path, &op[2])?;
            actual[2] = tip.clone();
            return Ok(Some((branch.trim().to_owned(), tip)));
        }
        Ok(None)
    }

    fn dispatch(&self, step: &model::PlannedStep<'_>, actual: &[String]) -> Result<String, String> {
        let op = step.argv;
        if let [_, original @ ..] = op
            && step.kind == model::Step::GerritAmend
        {
            let prepared = crate::gerrit::amend_args(self.path, original)?;
            let args: Vec<&str> = prepared.iter().map(String::as_str).collect();
            return run_step(self.path, &args, self.creds, self.quiet, self.control);
        }
        if step.kind == model::Step::ReviewedPush {
            return self.dispatch_reviewed(op);
        }
        if let [_, id, fix] = op
            && step.kind == model::Step::PreflightFix
        {
            let plan = crate::push_preflight::registered(id)?;
            return crate::push_preflight::fix(self.path, &plan, fix);
        }
        let args: Vec<&str> = actual.iter().map(String::as_str).collect();
        run_step(self.path, &args, self.creds, self.quiet, self.control)
    }

    fn dispatch_reviewed(&self, op: &[String]) -> Result<String, String> {
        let (id, evidence) = reviewed_operands(op)?;
        let plan = crate::push_preflight::registered(id)?;
        let original = crate::push_preflight::push_args(&plan)?;
        let source = evidence
            .map(|args| git::CommandSource::reviewed_binding(self.path, args))
            .transpose()?;
        run_reviewed_push(
            &original,
            |args| git::RouteBinding::capture(self.path, args),
            || match source {
                Some(source) => crate::push_preflight::execute_controlled_with_guard(
                    self.path,
                    &plan,
                    self.creds,
                    self.control,
                    || source.check_execution(),
                ),
                None => crate::push_preflight::execute_controlled(
                    self.path,
                    &plan,
                    self.creds,
                    self.control,
                ),
            },
            self.control,
        )
    }
}
