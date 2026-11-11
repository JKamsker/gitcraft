//! Validate a complete immutable plan before repository preparation and dispatch.
use super::*;

impl<'a> Execution<'a> {
    pub(super) fn prepare(
        path: &'a Path,
        ops: &'a [Vec<String>],
        kind: &OpKind,
        creds: Option<&'a git::Credentials>,
        quiet: bool,
        control: Option<&'a Arc<git::OpControl>>,
        fork_scope: Option<&'a Scope>,
    ) -> Result<Self, String> {
        check_command_envelopes(ops, matches!(kind, OpKind::Redo(_)), &mut |args| {
            git::run(Some(path), args)
        })?;
        if let Some(scope) = fork_scope {
            scope.reset_rewrite();
        }
        let saved_label = match kind {
            OpKind::Undo(entry) | OpKind::Redo(entry) => Some(entry.label.as_str()),
            OpKind::Normal(..) => None,
        };
        let checkout = CheckoutReplay::read(ops, saved_label)?;
        let expected = expected_head(kind);
        if let Some(expected) = expected {
            verify_head(path, expected)?;
        }
        // Read the actual HEAD, rather than a previous UI reload's snapshot.
        let head_before =
            matches!(kind, OpKind::Normal(Undo::HeadMove { .. }, _)).then(|| git::head(path));
        // Preconditions and earlier read-only steps do not hide a stash push.
        let pushes_stash = ops.iter().any(|op| {
            matches!(op.as_slice(), [stash, push, ..]
            if stash == "stash" && push == "push")
        });
        let stash_before = pushes_stash.then(|| git::resolve(path, "refs/stash").ok());
        let approved_head = expected.or(head_before.as_deref());
        with_rechecked_preparation(
            || {
                check_https_routes(ops, |binding| binding.verify(path))?;
                check_expected(path, ops, control)?;
                if let Some(expected) = approved_head
                    && git::head(path) != expected
                {
                    return Err(crate::i18n::text("repo-core-4166").into());
                }
                Ok(())
            },
            || capture_draft(path, ops, control),
        )?;
        Ok(Self {
            path,
            ops,
            creds,
            quiet,
            control,
            fork_scope,
            checkout,
            head_before,
            stash_before,
            guarded_fork: ops.iter().any(|op| crate::repo::forks::guard::is_guard(op)),
            owned_refs: Vec::new(),
            rewrite: None,
            pushed: Vec::new(),
            tried: Vec::new(),
        })
    }
}

fn expected_head(kind: &OpKind) -> Option<&str> {
    match kind {
        OpKind::Undo(entry) => entry.heads.as_ref().map(|(_, after)| after.as_str()),
        OpKind::Redo(entry) => entry.heads.as_ref().map(|(before, _)| before.as_str()),
        OpKind::Normal(..) => None,
    }
}

fn verify_head(path: &Path, expected: &str) -> Result<(), String> {
    let now = git::head(path);
    if now == expected {
        return Ok(());
    }
    Err(crate::i18n::message(
        "repo-core-4164",
        &[
            ("now", crate::repo::short(&now).to_string().into()),
            ("expected", crate::repo::short(expected).to_string().into()),
        ],
    ))
}

fn capture_draft(
    path: &Path,
    ops: &[Vec<String>],
    control: Option<&Arc<git::OpControl>>,
) -> Result<(), String> {
    let needed = ops.iter().any(|op| {
        git::subcommand(&op.iter().map(String::as_str).collect::<Vec<_>>())
            .is_some_and(|command| matches!(command, "checkout" | "switch" | "rebase"))
    });
    if !needed {
        return Ok(());
    }
    let active = || {
        !crate::tasks::cancellation().is_cancelled()
            && !control.is_some_and(|control| control.is_cancelled())
    };
    let snapshot = (|| {
        if git::drafts::enabled_while(path, &active)? {
            git::drafts::capture(path, crate::i18n::text("repo-core-4167"), &active)?;
        }
        Ok::<_, String>(())
    })();
    snapshot.map_err(|error| crate::i18n::message("repo-core-4168", &[("error", error.into())]))
}
