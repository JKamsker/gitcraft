use super::*;

/// What each did is kept in `control`, for the activity log.
pub(in crate::repo) fn run_step(
    path: &Path,
    args: &[&str],
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    let Some(control) = control else {
        return if quiet {
            git::run_unattended(Some(path), args, creds)
        } else {
            git::run_naming_hooks(Some(path), args, creds)
        };
    };
    if control.is_cancelled() {
        return Err(git::CANCELLED.to_owned());
    }
    let binding = if git::goes_over_network(args) {
        match git::RouteBinding::capture(path, args) {
            Ok(binding) => Some(binding),
            Err(error) => {
                return Err(crate::i18n::message(
                    "repo-core-2967",
                    &[(
                        "error",
                        crate::diagnostics::crash::scrub(&error).to_string().into(),
                    )],
                ));
            }
        }
    } else {
        None
    };
    let endpoints = binding
        .as_ref()
        .map(|b| b.urls().to_vec())
        .unwrap_or_default();
    // Each call kept for the activity log.
    let (result, recorded) = if git::goes_over_network(args) {
        git::run_watched_recorded(Some(path), args, creds, quiet, control)
    } else {
        git::run_recorded(Some(path), args, creds, quiet)
    };
    if let Err(error) = &result
        && git::is_auth_error(error)
    {
        let mut failure = git::AuthFailure::from_error(error, &endpoints);
        failure.binding = binding;
        control.authentication_failed(failure);
    }
    let mut recorded = recorded;
    recorded.output = crate::repo::activity::bounded(&recorded.output);
    control.record(recorded);
    result
}

/// Reviewed dispatch bypasses run_step's normal argv, but retains its original-remote
/// route proof. The runtime private alias must never become the retry authorization.
pub(in crate::repo) fn reviewed_operands(
    op: &[String],
) -> Result<(&str, Option<&[String]>), String> {
    match op {
        [marker, id] if marker == crate::push_preflight::APPROVED => Ok((id, None)),
        [marker, id, option, binding]
            if marker == crate::push_preflight::APPROVED
                && option == "-c"
                && binding.starts_with("gitcraft.commandSource=") =>
        {
            Ok((id, Some(&op[2..])))
        }
        _ => Err(crate::i18n::text("repo-core-2969").into()),
    }
}

/// Retrieve the immutable exact argv behind APPROVED; the private marker is
/// still interpreted only here, with its original strict operands.
pub(in crate::repo) fn reviewed_push_args(op: &[String]) -> Result<Option<Vec<String>>, String> {
    if op
        .first()
        .is_none_or(|mark| mark != crate::push_preflight::APPROVED)
    {
        return Ok(None);
    }
    let (id, _) = reviewed_operands(op)?;
    let plan = crate::push_preflight::registered(id)?;
    crate::push_preflight::push_args(&plan).map(Some)
}

pub(in crate::repo) fn retain_unfinished(ops: &mut Vec<Vec<String>>, stopped: usize) {
    // The checks and opaque source owner stay with the original operation on sign-in retry.
    let mut i = 0;
    ops.retain(|step| {
        i += 1;
        i > stopped || is_expectation(step) || step.first().is_some_and(|m| m == EXPECT_HTTPS_ROUTE)
    });
}

#[cfg(test)]
#[test]
pub(in crate::repo) fn merge_pure_reviewed_echo_operands_are_strict_and_survive_retry()
-> Result<(), String> {
    let plain = vec![crate::push_preflight::APPROVED.to_owned(), "plan".into()];
    assert_eq!(reviewed_operands(&plain)?, ("plan", None));
    let bound = [
        plain.clone(),
        vec!["-c".into(), "gitcraft.commandSource=owned".into()],
    ]
    .concat();
    let (id, source) = reviewed_operands(&bound)?;
    assert_eq!(id, "plan");
    assert_eq!(source, Some(&bound[2..]));
    for malformed in [
        [plain.clone(), vec!["-c".into()]].concat(),
        [
            plain.clone(),
            vec!["-c".into(), "http.proxy=foreign".into()],
        ]
        .concat(),
        [bound.clone(), vec!["extra".into()]].concat(),
    ] {
        assert!(reviewed_operands(&malformed).is_err());
    }
    // Credential recovery retains internal steps, including their opaque source owner.
    let mut retained = vec![bound.clone()];
    retain_unfinished(&mut retained, 0);
    assert_eq!(retained, vec![bound]);
    Ok(())
}

pub(crate) fn run_reviewed_push(
    original: &[String],
    capture: impl FnOnce(&[&str]) -> Result<git::RouteBinding, String>,
    execute: impl FnOnce() -> Result<String, String>,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    let context = crate::connections::current();
    let args: Vec<_> = original.iter().map(String::as_str).collect();
    let binding = capture(&args).map_err(|error| {
        crate::i18n::message(
            "repo-core-4157",
            &[(
                "error",
                crate::diagnostics::crash::scrub(&error).to_string().into(),
            )],
        )
    })?;
    let result = execute();
    if let Err(error) = &result
        && let Some(failure) = reviewed_auth_failure(error, binding, context)
        && let Some(control) = control
    {
        control.authentication_failed(failure);
    }
    result
}

/// A one-shot reviewed receipt is trusted only inside the captured original route.
/// Never parse the opaque diagnostic as a canonical Git URL or guess a fetch URL.
pub(in crate::repo) fn reviewed_auth_failure(
    error: &str,
    binding: git::RouteBinding,
    context: crate::connections::Context,
) -> Option<git::AuthFailure> {
    let endpoint = crate::push_preflight::take_auth_endpoint(error)?;
    if !binding.authorizes_endpoint(&endpoint) {
        return None;
    }
    Some(git::AuthFailure {
        context,
        endpoints: vec![endpoint],
        confirmed: true,
        binding: Some(binding),
        lfs: false,
    })
}

/// Route proofs participate in the same before/after-draft verifier as HEAD and refs.
pub(in crate::repo) fn check_https_routes(
    ops: &[Vec<String>],
    mut verify: impl FnMut(&git::RouteBinding) -> Result<(), String>,
) -> Result<(), String> {
    let plan = model::Plan::decode(ops);
    for json in plan.routes() {
        let binding = serde_json::from_str::<git::RouteBinding>(json)
            .map_err(|_| crate::i18n::text("repo-core-4158").to_owned())?;
        verify(&binding)?;
    }
    Ok(())
}

/// What a cancelled operation says: that it was, with what it may have left behind: the lock
/// files a stopped git couldn't remove (not those `before` it started: they are someone
/// else's), and for a pull, a merge or rebase it had started.
pub(in crate::repo) fn cancelled_message(
    path: &Path,
    ops: &[Vec<String>],
    before: &[std::path::PathBuf],
) -> String {
    let mut msg = git::CANCELLED.to_owned();
    let locks: Vec<String> = git::leftover_locks(path)
        .into_iter()
        .filter(|l| !before.contains(l))
        .map(|l| {
            l.strip_prefix(path)
                .map_or_else(|_| l.display().to_string(), |r| r.display().to_string())
        })
        .collect();
    if !locks.is_empty() {
        let (them, are) = if locks.len() == 1 {
            ("it", "is")
        } else {
            ("them", "are")
        };
        msg.push_str(&format!(
            ", but git left {} behind\nUntil {them} {are} deleted, git refuses to work in this \
             repository. Delete {them} if no other git program is running here.",
            locks.join(", ")
        ));
    }
    let pulls = ops.iter().any(|op| {
        op.iter().any(|w| w == "pull")
            && git::goes_over_network(&op.iter().map(String::as_str).collect::<Vec<_>>())
    });
    if pulls {
        msg.push_str(
            "\nIf the pull had started merging or rebasing, that is left in progress: Abort in \
             the toolbar goes back to where the branch was.",
        );
    }
    msg
}

/// Why `ops` don't run now: one of them goes on the network, and offline mode is on.
pub(in crate::repo) fn offline_refusal(ops: &[Vec<String>]) -> Option<String> {
    let pause = crate::network::refused()?;
    let network = ops.iter().any(|op| reaches_a_remote(op));
    network
        .then(|| crate::i18n::message("repo-core-4161", &[("why", pause.why().to_string().into())]))
}

/// Whether the git command `op` reaches a remote: fetch, pull, push, clone, `ls-remote`, the
/// `remote` subcommands that ask it (`update`, `prune`, `show` without `-n`) and LFS's
/// transfers. Not `remote add` / `rename` / `set-url` / `remove`, which only write the config,
/// nor `submodule update`, which reaches one only when a commit is missing (and then says so).
pub(crate) fn reaches_a_remote(op: &[String]) -> bool {
    if op
        .first()
        .is_some_and(|s| s == crate::push_preflight::APPROVED)
    {
        return true;
    }
    // Shared syntax does not choose network policy. A value such as
    // `--git-dir push` is never a command.
    let Ok((at, command)) = git::parse_command_envelope(op) else {
        return false;
    };
    let sub = Some(command);
    let rest: Vec<&str> = op[at + 1..].iter().map(String::as_str).collect();
    match sub {
        Some("fetch" | "pull" | "push" | "clone" | "ls-remote" | git::dvc::PULL_ALIAS) => true,
        Some("remote") => match rest.first().copied() {
            Some("update" | "prune") => true,
            Some("show") => !rest.contains(&"-n"),
            _ => false,
        },
        Some("lfs") => matches!(rest.first().copied(), Some("fetch" | "pull" | "push")),
        _ => false,
    }
}
