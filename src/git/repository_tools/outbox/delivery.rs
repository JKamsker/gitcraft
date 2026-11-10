//! Outbox delivery preparation, consent checks, dispatch, and removal.

use super::*;

pub(crate) fn prepare(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, credentials, false)
}

pub(crate) fn review_prepare(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, credentials, true)
}

pub(super) fn prepare_with(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
    review: bool,
) -> Result<Prepared, String> {
    let context = crate::connections::current();
    let consent = match Consent::capture(&profile, &context, credentials) {
        Ok(consent) => consent,
        Err(error) => {
            pause(repo, id, &error)?;
            return Err(error);
        }
    };
    let mut view = read(repo)?;
    if review {
        let e = view
            .queue
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(crate::i18n::text("backend-entry-disappeared-c60598"))?;
        if !matches!(e.status, Status::Paused(_) | Status::Sending) {
            return Err(crate::i18n::text(
                "backend-only-paused-or-uncertain-entries-need-renewed-approval-233504",
            )
            .into());
        }
        let (remote, destination, stamp, endpoints) = routing(repo, &e.source)?;
        e.reviewed_tip = text(
            repo,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}^{{commit}}", e.source),
            ],
        )?
        .trim()
        .into();
        e.remote = remote;
        e.destination = destination;
        e.routing = stamp;
        e.endpoints = endpoints;
        e.status = Status::Ready;
        e.autosend = false;
        e.consent = Some(consent.clone());
    }
    let e = entry(&view, id)?;
    if !review
        && let Err(error) =
            Consent::require(e.consent.as_ref(), &consent).and_then(|_| live_check(e, &context))
    {
        pause(repo, id, &error)?;
        return Err(error);
    }
    if e.status != Status::Ready {
        return Err(crate::i18n::text(
            "backend-entry-is-paused-in-flight-or-sent-inspect-its-receipt-queue-a-new-d6598c",
        )
        .into());
    }
    if let Err(error) = check(repo, e) {
        pause(repo, id, &error)?;
        return Err(error);
    }
    let ops = vec![vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        "--recurse-submodules=check".into(),
        "--".into(),
        e.remote.clone(),
        format!("{}:{}", e.commit, e.destination),
    ]];
    let plan = match preflight::scan(repo, ops, profile, &|| {
        !crate::tasks::cancellation().is_cancelled()
    }) {
        Ok(p) => p,
        Err(error) => {
            pause(repo, id, &error)?;
            return Err(error);
        }
    };
    if plan.head != e.commit
        || plan.remote != e.remote
        || plan.destination != e.destination
        || plan.force
        || plan.deleting
    {
        return Err(crate::i18n::text(
            "backend-preflight-differs-from-the-exact-queued-plain-push-8d6c3d",
        )
        .into());
    }
    Ok(Prepared {
        view,
        id: id.into(),
        plan,
        review,
        context,
        consent,
        ui_generation: None,
    })
}

pub(super) fn pause(repo: &Path, id: &str, why: &str) -> Result<(), String> {
    let mut view = read(repo)?;
    let e = view
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(crate::i18n::text("backend-entry-disappeared-c60598"))?;
    e.status = Status::Paused(git::redact(why, None));
    save(repo, &view, None)
}

pub(crate) fn send(
    runner: &Runner<'_>,
    p: &Prepared,
    automatic: bool,
    reviewed: &preflight::ReviewedSend,
) -> Result<String, String> {
    reviewed.check_plan(&p.plan)?;
    if crate::network::refused().is_some() {
        return Err(crate::i18n::text("backend-outbox-remains-queued-while-offline-7f7902").into());
    }
    let mut view = read(runner.repo)?;
    if view.version != p.view.version {
        return Err(crate::i18n::text("backend-outbox-changed-review-again-6c3b43").into());
    }
    let actual = entry(&view, &p.id)?;
    if p.review && (!matches!(actual.status, Status::Paused(_) | Status::Sending) || automatic) {
        return Err(crate::i18n::text("backend-renewed-approval-no-longer-applies-544f7d").into());
    }
    let e = if p.review {
        entry(&p.view, &p.id)?.clone()
    } else {
        actual.clone()
    };
    if e.status != Status::Ready {
        return Err(crate::i18n::text("backend-this-entry-is-no-longer-ready-35173b").into());
    }
    let selected = match Consent::capture(
        &p.plan.profile,
        &crate::connections::current(),
        runner.credentials,
    ) {
        Ok(consent) => consent,
        Err(error) => {
            pause(runner.repo, &e.id, &error)?;
            return Err(error);
        }
    };
    if let Err(error) = Consent::require(e.consent.as_ref(), &selected)
        .and_then(|_| {
            if p.context == crate::connections::current() && p.consent == selected {
                Ok(())
            } else {
                Err(crate::i18n::text(
                    "backend-prepared-connection-changed-prepare-and-approve-again-562fe9",
                )
                .into())
            }
        })
        .and_then(|_| {
            if p.review {
                Ok(())
            } else {
                live_check(&e, &p.context)
            }
        })
    {
        pause(runner.repo, &e.id, &error)?;
        return Err(error);
    }
    if automatic && (!e.autosend || !p.plan.findings.is_empty()) {
        pause(
            runner.repo,
            &e.id,
            crate::i18n::text(
                "backend-automatic-plain-send-requires-a-clean-preflight-review-its-findin-85a272",
            ),
        )?;
        return Err(
            crate::i18n::text("backend-automatic-send-paused-for-preflight-review-e64ade").into(),
        );
    }
    if let Err(error) = check(runner.repo, &e) {
        pause(runner.repo, &e.id, &error)?;
        return Err(error);
    }
    preflight::check(runner.repo, &p.plan)?;
    // Durable before network I/O. A crash/cancel/lost acknowledgement stays Sending and is
    // explicitly ineligible for retry, including after application restart.
    let slot = view
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == p.id)
        .ok_or("Entry disappeared")?;
    *slot = e.clone();
    slot.status = Status::Sending;
    save(runner.repo, &view, None)?;
    live_consents()
        .lock()
        .map_err(|_| {
            crate::i18n::text(
                "backend-sending-intent-saved-live-consent-unavailable-inspect-before-retr-cb5284",
            )
        })?
        .insert((e.identity.clone(), e.id.clone()), p.context.clone());
    let result = preflight::execute_bound_controlled(
        runner.repo,
        &p.plan,
        reviewed,
        runner.credentials,
        Some(runner.control),
    );
    let mut receipt = read(runner.repo)?;
    let e = receipt
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == p.id)
        .ok_or(crate::i18n::text(
            "backend-push-ended-but-entry-changed-inspect-the-remote-before-another-se-875973",
        ))?;
    match &result {
        Ok(output) => {
            e.status = Status::Sent;
            e.receipt = git::redact(output, None);
        }
        Err(error) => {
            e.status=Status::Paused("Push rejected, cancelled or outcome uncertain. Inspect the destination before queueing again.".into());
            e.receipt = git::redact(error, None);
        }
    }
    let sent_commit = e.commit.clone();
    save(runner.repo, &receipt, None).map_err(|_| {
        crate::i18n::text(
            "backend-push-attempted-but-receipt-could-not-be-saved-inspect-the-destina-704969",
        )
        .to_owned()
    })?;
    result.map(|_| {
        crate::i18n::message(
            "backend-published-exact-queued-commit-sent-commit-04c0e8",
            &[("sent_commit", (sent_commit).to_string().into())],
        )
    })
}

pub(crate) fn remove(repo: &Path, expected: &View, id: &str) -> Result<String, String> {
    if read(repo)?.version != expected.version {
        return Err(crate::i18n::text("backend-outbox-changed-reopen-it-014048").into());
    }
    let e = entry(expected, id)?;
    if e.status == Status::Sending {
        return Err(crate::i18n::text(
            "backend-outcome-uncertain-inspect-the-remote-and-retain-this-receipt-no-r-36e9d1",
        )
        .into());
    }
    let mut view = expected.clone();
    view.queue.entries.retain(|e| e.id != id);
    save(repo, &view, None)?;
    if let Ok(mut live) = live_consents().lock() {
        live.remove(&(e.identity.clone(), e.id.clone()));
    }
    Ok(crate::i18n::text(
        "backend-removed-queue-entry-protected-source-commit-is-retained-for-recov-91464b",
    )
    .into())
}
