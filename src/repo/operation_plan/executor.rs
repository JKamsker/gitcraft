use super::*;

/// [`run_steps_counted`] without where it stopped.
#[cfg(test)]
pub(in crate::repo) fn run_steps(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
) -> (Result<String, String>, OpKind) {
    let (result, kind, _) = run_steps_counted(path, ops, kind, creds, false, None);
    (result, kind)
}

/// Native test harnesses must interpret NUL checkout markers through the same
/// guarded worker as the application, without exposing internal operation kinds.
#[cfg(test)]
pub(crate) fn run_live_checkout(
    path: &Path,
    ops: &[Vec<String>],
    creds: Option<&git::Credentials>,
) -> Result<String, String> {
    run_steps(path, ops, OpKind::plain(), creds).0
}

/// Runs git commands one after another, stopping at the first failure; `kind` comes back with
/// what undoing needs, as found on disk. Also says which step it stopped at (`ops.len()` when
/// all ran): the steps before it are done, and running the operation again (after signing in)
/// starts there. A `quiet` one asks nobody for credentials ([`git::run_unattended`]).
pub(in crate::repo) fn run_steps_counted(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
) -> (Result<String, String>, OpKind, usize) {
    run_steps_guarded(path, ops, kind, creds, quiet, control, None)
}

pub(in crate::repo) fn run_steps_guarded(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
    fork_scope: Option<&crate::repo::forks::guard::Scope>,
) -> (Result<String, String>, OpKind, usize) {
    // Validate the complete saved envelope before preparation or any mutating step.
    if let Err(error) = check_command_envelopes(ops, matches!(kind, OpKind::Redo(_)), &mut |args| {
        git::run(Some(path), args)
    }) {
        return (Err(error), kind, 0);
    }
    if let Some(scope) = fork_scope {
        scope.reset_rewrite();
    }
    let saved_label = match &kind {
        OpKind::Undo(entry) | OpKind::Redo(entry) => Some(entry.label.as_str()),
        OpKind::Normal(..) => None,
    };
    let mut checkout = match crate::repo::forks::guard::CheckoutReplay::read(ops, saved_label) {
        Ok(checkout) => checkout,
        Err(e) => return (Err(e), kind, 0),
    };
    let mut owned_refs = Vec::new();
    let mut rewrite = None;
    let guarded_fork = ops.iter().any(|op| crate::repo::forks::guard::is_guard(op));
    let mut pushed = Vec::new();
    // Undoing a HEAD move resets the branch checked out now: after a checkout or a commit made
    // elsewhere since, that would throw away commits that aren't the operation's.
    let expected = match &kind {
        OpKind::Undo(entry) => entry.heads.as_ref().map(|(_, after)| after),
        OpKind::Redo(entry) => entry.heads.as_ref().map(|(before, _)| before),
        OpKind::Normal(..) => None,
    };
    if let Some(expected) = expected {
        let now = git::head(path);
        if &now != expected {
            return (
                Err(crate::i18n::message(
                    "repo-core-4164",
                    &[
                        ("now", crate::repo::short(&now).to_string().into()),
                        ("expected", crate::repo::short(expected).to_string().into()),
                    ],
                )),
                kind,
                0,
            );
        }
    }
    // Where HEAD really is now, not where the last reload saw it: a commit made in the
    // terminal since must not be undone along with this.
    let head_before =
        matches!(kind, OpKind::Normal(Undo::HeadMove { .. }, _)).then(|| git::head(path));
    // Any `git stash push`: of everything, of some files, of what is staged.
    let pushes_stash = ops
        .first()
        .is_some_and(|op| op.len() >= 2 && op[0] == "stash" && op[1] == "push");
    let stash_before = pushes_stash.then(|| git::resolve(path, "refs/stash").ok());
    let needs_draft = ops.iter().any(|op| {
        git::subcommand(&op.iter().map(String::as_str).collect::<Vec<_>>())
            .is_some_and(|s| matches!(s, "checkout" | "switch" | "rebase"))
    });
    let approved_head = expected.or(head_before.as_ref());
    let prepared = with_rechecked_preparation(
        || {
            check_https_routes(ops, |binding| binding.verify(path))?;
            check_expected(path, ops, control)?;
            if let Some(expected) = approved_head
                && git::head(path) != *expected
            {
                return Err(crate::i18n::text("repo-core-4166").into());
            }
            Ok(())
        },
        || {
            if !needs_draft {
                return Ok(());
            }
            let active = || {
                !crate::tasks::cancellation().is_cancelled()
                    && !control.is_some_and(|c| c.is_cancelled())
            };
            let snapshot = (|| {
                if git::drafts::enabled_while(path, &active)? {
                    git::drafts::capture(path, crate::i18n::text("repo-core-4167"), &active)?;
                }
                Ok::<_, String>(())
            })();
            snapshot.map_err(|error| {
                crate::i18n::message("repo-core-4168", &[("error", error.to_string().into())])
            })
        },
    );
    if let Err(error) = prepared {
        return (Err(error), kind, 0);
    }
    let mut result = Ok(String::new());
    let mut stopped = ops.len();
    let mut tried = Vec::new();
    let plan = model::Plan::decode(ops);
    for step in plan.steps() {
        let i = step.index;
        let op = step.argv;
        #[cfg(test)]
        if !is_expectation(op) {
            crate::repo::forks::before_step(op);
        }
        if let Err(e) = fork_scope
            .map_or(Ok(()), |scope| scope.check_remote(path))
            .and_then(|()| crate::repo::forks::guard::check(path, ops))
            .and_then(|()| checkout.as_mut().map_or(Ok(()), |c| c.check(path)))
            .and_then(|()| fork_scope.map_or(Ok(()), |scope| scope.check()))
        {
            result = Err(e);
            stopped = i;
            break;
        }
        if let model::Step::Expectation(model::Precondition::FoldSafe(target)) = &step.kind
            && let Err(error) =
                git::fold_read(path, target).and_then(crate::repo::fold::plan::Plan::from_read)
        {
            result = Err(error);
            stopped = i;
            break;
        }
        if let model::Step::Expectation(model::Precondition::FoldIgnored(target)) = &step.kind
            && let Err(error) =
                git::fold_read(path, target).and_then(crate::repo::fold::plan::Plan::check_ignored)
        {
            result = Err(error);
            stopped = i;
            break;
        }
        if matches!(step.kind, model::Step::Expectation(_)) {
            continue;
        }
        if let Err(error) = crate::repo::partial_stash::validate_push(path, op) {
            result = Err(error);
            stopped = i;
            break;
        }
        if let model::Step::Try { note: says, args } = &step.kind {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Err(e) = git::run_unattended(Some(path), &args, creds) {
                tried.push(format!("{says} ({})", headline(&e)));
            }
            continue;
        }
        let reviewed = match reviewed_push_args(op) {
            Ok(reviewed) => reviewed,
            Err(e) => {
                result = Err(e);
                stopped = i;
                break;
            }
        };
        let capture_args = reviewed.as_deref().unwrap_or(op);
        let prepared = match crate::repo::forks::guard::prepare_push(
            path,
            capture_args,
            guarded_fork,
            rewrite.as_ref(),
        ) {
            Ok(prepared) => prepared,
            Err(e) => {
                result = Err(e);
                stopped = i;
                break;
            }
        };
        // The preflight sender must execute exactly the argv this evidence describes.
        // A resolved APPROVED source that differs is stale, never silently substituted.
        if reviewed.is_some()
            && prepared
                .as_ref()
                .is_some_and(|(args, _)| args != capture_args)
        {
            result = Err("The reviewed fork source changed; reopen push preflight".into());
            stopped = i;
            break;
        }
        let mut actual = prepared
            .as_ref()
            .map_or_else(|| op.to_vec(), |(args, _)| args.clone());
        if actual.first().is_some_and(|arg| arg == "pull") {
            match ignored_safe_pull(path, &actual) {
                Ok(args) => actual = args,
                Err(error) => {
                    result = Err(error);
                    stopped = i;
                    break;
                }
            }
        }
        if command_envelope(&actual).is_ok_and(|(at, command)| {
            command == "submodule" && actual.get(at + 1).is_some_and(|arg| arg == "update")
        }) {
            let check = if actual.first().is_some_and(|arg| arg != "submodule") {
                Err("Submodule updates with global options cannot be validated safely. Run Update submodules again.".into())
            } else {
                check_ignored_submodule_update_with(&actual, |args, codes| {
                    git::run_ok_codes(Some(path), args, codes)
                })
            };
            if let Err(error) = check {
                result = Err(error);
                stopped = i;
                break;
            }
        }
        // Recheck immediately before invoking rebase. A rejected plan never owns a rewrite.
        let rebasing = (fork_scope.is_some() || guarded_fork)
            && matches!(op, [c, config, rebase, _] if c == "-c" && config == "rebase.updateRefs=false" && rebase == "rebase");
        let rewrite_before = if rebasing {
            if let Err(e) = check_expected(path, ops, control) {
                result = Err(e);
                stopped = i;
                break;
            }
            git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
                .ok()
                .map(|branch| (branch.trim().to_owned(), git::head(path)))
        } else {
            None
        };
        let resetting = matches!(op, [reset, mode, _] if reset == "reset" && mode == "--keep");
        let deleting = matches!(op, [branch, delete, ..] if branch == "branch" && delete == "-D");
        if guarded_fork
            && (resetting || deleting)
            && let Err(e) = check_local_expected(path, ops, op, control)
        {
            result = Err(e);
            stopped = i;
            break;
        }
        let reset_result = if resetting
            && ops.iter().any(|step| {
                expected_ref(step).is_some_and(|(name, _)| name.starts_with("refs/heads/"))
            })
            && let Ok(branch) = git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
        {
            match git::resolve(path, &op[2]) {
                Ok(tip) => {
                    actual[2] = tip.clone();
                    Some((branch.trim().to_owned(), tip))
                }
                Err(e) => {
                    result = Err(e);
                    stopped = i;
                    break;
                }
            }
        } else {
            None
        };
        let args: Vec<&str> = actual.iter().map(String::as_str).collect();
        if let Some(folder) = crate::agent::added_worktree(op)
            && let Err(e) = if control.is_some_and(|c| c.is_cancelled()) {
                Err(git::CANCELLED.to_owned())
            } else {
                git::prepare_worktree_folder(path, folder)
            }
        {
            result = Err(e);
            stopped = i;
            break;
        }
        result = (|| {
            if let [_, original @ ..] = op
                && step.kind == model::Step::GerritAmend
            {
                let prepared = crate::gerrit::amend_args(path, original)?;
                let args: Vec<&str> = prepared.iter().map(String::as_str).collect();
                return run_step(path, &args, creds, quiet, control);
            }
            if step.kind == model::Step::ReviewedPush {
                let (id, evidence) = reviewed_operands(op)?;
                let plan = crate::push_preflight::registered(id)?;
                let original = crate::push_preflight::push_args(&plan)?;
                let source = evidence
                    .map(|args| git::CommandSource::reviewed_binding(path, args))
                    .transpose()?;
                return run_reviewed_push(
                    &original,
                    |args| git::RouteBinding::capture(path, args),
                    || match source {
                        Some(source) => crate::push_preflight::execute_controlled_with_guard(
                            path,
                            &plan,
                            creds,
                            control,
                            || source.check_execution(),
                        ),
                        None => {
                            crate::push_preflight::execute_controlled(path, &plan, creds, control)
                        }
                    },
                    control,
                );
            }
            if let [_, id, fix] = op
                && step.kind == model::Step::PreflightFix
            {
                let plan = crate::push_preflight::registered(id)?;
                return crate::push_preflight::fix(path, &plan, fix);
            }
            run_step(path, &args, creds, quiet, control)
        })();
        if let Some((branch, before)) = rewrite_before {
            let done = crate::repo::forks::guard::Rewrite {
                tip: git::resolve(path, &branch).unwrap_or_default(),
                branch,
                before,
                conflicts: result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.contains("CONFLICT") || e.contains("could not apply")),
            };
            if let Some(scope) = fork_scope {
                scope.record_rewrite(done.clone());
            }
            if !done.tip.is_empty() {
                owned_refs.push((done.branch.clone(), done.tip.clone()));
            }
            rewrite = Some(done);
        }
        if result.is_ok() {
            if let Some(done) = reset_result {
                owned_refs.push(done);
            }
            if checkout.is_some()
                && op.first().is_some_and(|s| s == "checkout")
                && let Ok(branch) = git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
            {
                owned_refs.push((branch.trim().to_owned(), git::head(path)));
            }
            // Pin a new remote at its creating step, before marker/config/fetch/checkout.
            if matches!(op, [remote, add, ..] if remote == "remote" && add == "add") {
                result = fork_scope
                    .map_or(Ok(()), |scope| scope.check_remote(path))
                    .and_then(|()| checkout.as_mut().map_or(Ok(()), |c| c.check(path)))
                    .map(|()| String::new());
                if result.is_err() {
                    stopped = i + 1; // remote add completed; a retry must retain its guard, not add it twice.
                    break;
                }
            }
        }
        crate::repo::forks::guard::completed_push(&result, prepared, &mut pushed);
        if result.is_err() {
            stopped = i;
            break;
        }
    }
    if result.is_ok() && !tried.is_empty() {
        result = Ok(format!("{TRIED}{}", tried.join("; ")));
    }
    // "No local changes to save" makes no stash: undoing would pop an older one.
    let no_stash = stash_before.is_some_and(|b| b == git::resolve(path, "refs/stash").ok());
    let kind = match kind {
        // For HEAD moves, the new HEAD is what redo goes back to.
        OpKind::Normal(Undo::HeadMove { mode }, before) if result.is_ok() => {
            let before = head_before.filter(|h| !h.is_empty()).unwrap_or(before);
            // And the branch it moved, which undo and redo must find checked out again.
            let branch = git::run_ok_codes(Some(path), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            OpKind::Normal(
                Undo::HeadMove { mode },
                format!("{before}\u{0}{}\u{0}{branch}", git::head(path)),
            )
        }
        OpKind::Normal(Undo::Custom { .. }, before) if no_stash => {
            OpKind::Normal(Undo::None, before)
        }
        // The way back expects the refs where this left them.
        OpKind::Normal(Undo::Custom { mut undo, mut redo }, before) if result.is_ok() => {
            fill_expected(path, &mut undo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut undo);
                c.bind(&mut redo);
            }
            crate::repo::forks::guard::bind_leases(&mut undo, &pushed);
            OpKind::Normal(Undo::Custom { undo, redo }, before)
        }
        OpKind::Undo(mut entry) if result.is_ok() => {
            fill_expected(path, &mut entry.redo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut entry.undo);
                c.bind(&mut entry.redo);
            }
            crate::repo::forks::guard::bind_leases(&mut entry.redo, &pushed);
            OpKind::Undo(entry)
        }
        // Redone with nothing to stash, the undo keeps expecting the stash it popped, and so
        // refuses rather than pop an older one.
        OpKind::Redo(mut entry) if result.is_ok() && !no_stash => {
            fill_expected(path, &mut entry.undo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut entry.undo);
                c.bind(&mut entry.redo);
            }
            crate::repo::forks::guard::bind_leases(&mut entry.undo, &pushed);
            OpKind::Redo(entry)
        }
        other => other,
    };
    (result, kind, stopped)
}
