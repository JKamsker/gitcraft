//! Operation orchestration facade. Persisted argv is decoded at the execution boundary.
use super::messages::spawn_mutation;
use super::notice::operation::{Failure, Status as OperationStatus};
use super::toolbar::headline;
use super::{Dialog, Msg, OpKind, PendingOp, Refresh, RepoTab, Undo, UndoEntry, ops};
use crate::git;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
#[path = "operation_plan/model.rs"]
mod model;
const EXPECT_HTTPS_ROUTE: &str = "--gitcraft-expect-https-route";
pub(super) const LONG_OP: Duration = Duration::from_secs(4);
#[path = "operation_plan/start.rs"]
mod start;
#[allow(unused_imports)]
pub(crate) use start::*;
#[path = "operation_plan/credentials.rs"]
mod credentials;
#[allow(unused_imports)]
pub(crate) use credentials::*;
#[path = "operation_plan/completion.rs"]
mod completion;
#[allow(unused_imports)]
pub(crate) use completion::*;
#[path = "operation_plan/dispatch.rs"]
mod dispatch;
#[allow(unused_imports)]
pub(crate) use dispatch::*;
#[path = "operation_plan/executor.rs"]
mod executor;
#[allow(unused_imports)]
pub(crate) use executor::*;
#[path = "operation_plan/expectations.rs"]
mod expectations;
#[allow(unused_imports)]
pub(crate) use expectations::*;
#[path = "operation_plan/envelope.rs"]
mod envelope;
#[allow(unused_imports)]
pub(crate) use envelope::*;
#[path = "operation_plan/pull_options.rs"]
mod pull_options;
#[allow(unused_imports)]
pub(crate) use pull_options::*;
#[path = "operation_plan/ignored_files.rs"]
mod ignored_files;
#[allow(unused_imports)]
pub(crate) use ignored_files::*;
#[path = "operation_plan/preconditions.rs"]
mod preconditions;
#[allow(unused_imports)]
pub(crate) use preconditions::*;
#[cfg(test)]
#[path = "operation_plan/tests.rs"]
mod tests;

#[cfg(test)]
mod signout_tests {
    use super::*;
    #[test]
    fn signout_clears_nested_parked_credentials_before_a_view_is_reopened() -> Result<(), String> {
        let ctx = egui::Context::default();
        let url = "https://push-a.invalid/team/repo.git";
        let mut main = RepoTab::new_deferred("/nonexistent/signout-main".into(), &ctx);
        let mut parked = RepoTab::new_deferred("/nonexistent/signout-worktree".into(), &ctx);
        let mut nested = RepoTab::new_deferred("/nonexistent/signout-nested".into(), &ctx);
        let unrelated = git::Credentials::for_url(
            "https://push-b.invalid/team/repo.git",
            "other".into(),
            "DUMMY_OTHER".into(),
        )?;
        main.credentials = Some(unrelated);
        parked.credentials = Some(git::Credentials::for_url(
            url,
            "old-account".into(),
            "DUMMY_OLD".into(),
        )?);
        nested.credentials = parked.credentials.clone();
        parked.parked.push(Box::new(nested));
        let path = parked.path.clone();
        main.parked.push(Box::new(parked));
        main.forget_https_memory(std::path::Path::new("/nonexistent/another-tab"), url);
        assert!(main.credentials.is_some());
        let reopened = main.take_parked(&path).ok_or("Parked view missing")?;
        assert!(reopened.credentials.is_none());
        assert!(reopened.parked[0].credentials.is_none());
        Ok(())
    }
}

#[cfg(test)]
mod core_guard_paint_tests {
    use super::*;
    use crate::repo::push_preflight::core_guard_paint_tests::paint;

    #[test]
    fn core_guard_paint_reviewed_auth_extracts_original_proof_before_display() -> Result<(), String>
    {
        let binding: git::RouteBinding = serde_json::from_value(serde_json::json!({
            "args": ["push", "--", "remote-12345", "HEAD:refs/heads/topic-12345"],
            "routes": ["https://push.invalid/team/12345"],
            "configuration": "opaque-configuration-12345"
        }))
        .map_err(|error| error.to_string())?;
        let original = binding.args.clone();
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            let receipt =
                crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/12345");
            let before = receipt.clone();
            let control = git::OpControl::new(|| {});
            let captured = std::cell::Cell::new(false);
            let result = run_reviewed_push(
                &original,
                |args| {
                    assert_eq!(
                        args,
                        original.iter().map(String::as_str).collect::<Vec<_>>()
                    );
                    captured.set(true);
                    Ok(binding.clone())
                },
                || {
                    assert!(captured.get());
                    Err(receipt.clone())
                },
                Some(&control),
            );
            assert_eq!(result, Err(before.clone()));
            let failure = control.auth_failure().ok_or("typed authentication proof")?;
            assert!(failure.confirmed);
            assert_eq!(failure.binding, Some(binding.clone()));
            assert_eq!(failure.endpoints, ["https://push.invalid/team/12345"]);
            assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
            // Presentation follows actual one-shot extraction; no receipt is sent to a translator.
            let shown = paint(&egui::Context::default(), |ui| {
                ui.label(crate::push_preflight::display::reviewed_auth_failure());
            });
            assert!(shown.contains(if locale == "de" {
                "Authentifizierung"
            } else {
                "Authentication"
            }));
            assert!(!shown.contains("gitcraft-reviewed-auth"));
            assert!(crate::push_preflight::is_reviewed_auth_error(&receipt));
            assert_eq!(receipt, before);
            assert_eq!(crate::push_preflight::APPROVED, "\0push-preflight");
            assert_eq!(crate::push_preflight::FIX, "\0push-preflight-fix");
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn recording_approved_fork_capture_binds_only_successful_exact_pushed_tips() -> Result<(), String> {
    use super::forks::guard;
    const UNBOUND: &str = "\0fork-pushed-tip";
    let expected = "a".repeat(40);
    let tip = "b".repeat(40);
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: tip.clone(),
        remote: "fork".into(),
        destination: "refs/heads/topic".into(),
        urls: vec!["https://forge.invalid/team/fork".into()],
        lease: Some(expected.clone()),
    };
    let plan = crate::push_preflight::selected_continuation_recording::plan(&route);
    let original = crate::push_preflight::push_args(&plan)?;
    let approved = crate::push_preflight::approved_ops(plan);
    let marker = approved.last().ok_or("missing APPROVED marker")?;
    assert_eq!(marker[0], crate::push_preflight::APPROVED);
    let captured = reviewed_push_args(marker)?.ok_or("missing APPROVED capture")?;
    assert_eq!(captured, original);
    assert!(captured.contains(&format!("--force-with-lease=refs/heads/topic:{expected}")));
    let prepared = guard::prepare_push_with(&captured, true, None, |source| {
        assert_eq!(source, tip);
        Ok(tip.clone())
    })?
    .ok_or("reviewed fork source evidence missing")?;
    assert_eq!(prepared.0, original);
    for lease in [
        "--force-with-lease=refs/heads/topic:",
        "--force-with-lease=refs/heads/topic",
    ] {
        let mut unsupported_fork = captured.clone();
        let at = unsupported_fork
            .iter()
            .position(|arg| arg.starts_with("--force-with-lease="))
            .ok_or("missing fixed lease")?;
        unsupported_fork[at] = lease.into();
        let resolved = std::cell::Cell::new(false);
        assert!(
            guard::prepare_push_with(&unsupported_fork, true, None, |_| {
                resolved.set(true);
                Err("unexpected resolution".into())
            })
            .is_err()
        );
        assert!(
            !resolved.get(),
            "fork lease must remain fixed before resolving"
        );
    }
    let inverse = vec![vec![
        "push".into(),
        format!("--force-with-lease=refs/heads/topic:{}", UNBOUND),
        "--".into(),
        "fork".into(),
        format!("{expected}:refs/heads/topic"),
    ]];
    for result in [
        Ok(String::new()),
        Err("auth failure".into()),
        Err(git::CANCELLED.into()),
        Err("stale guard".into()),
    ] {
        let mut pushed = Vec::new();
        guard::completed_push(&result, Some(prepared.clone()), &mut pushed);
        let mut undo = inverse.clone();
        guard::bind_leases(&mut undo, &pushed);
        assert_eq!(pushed.len(), usize::from(result.is_ok()));
        let lease = if result.is_ok() {
            tip.as_str()
        } else {
            UNBOUND
        };
        assert_eq!(
            undo[0][1],
            format!("--force-with-lease=refs/heads/topic:{lease}")
        );
        // An error never turns the unresolved inverse into an executable push.
        if result.is_err() {
            let resolved = std::cell::Cell::new(false);
            assert!(
                guard::prepare_push_with(&undo[0], true, None, |_| {
                    resolved.set(true);
                    Err("unexpected resolution".into())
                })
                .is_err()
            );
            assert!(!resolved.get(), "invalid lease must not resolve");
        }
    }
    let mut other = inverse.clone();
    other[0][3] = "another-fork".into();
    let mut pushed = Vec::new();
    guard::completed_push(&Ok(String::new()), Some(prepared), &mut pushed);
    guard::bind_leases(&mut other, &pushed);
    assert_eq!(other[0][1], inverse[0][1]);
    let mut malformed = marker.clone();
    malformed.push("extra".into());
    assert!(reviewed_push_args(&malformed).is_err());
    Ok(())
}

#[path = "operation_plan/grammar_adapter.rs"]
mod grammar_adapter;
use grammar_adapter::*;
