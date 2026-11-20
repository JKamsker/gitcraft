//! The badge of a real tab: what the checkout, binding and connection make of an answer, and
//! when reads are submitted.
use std::time::Instant;

use super::binding::Binding;
use super::*;
use crate::analysis::Target;
use crate::analysis::gate::{Analysis, ServerGate, Stale};
use crate::analysis::sonar::Account;
use crate::connections::{Choices, Context};

const HEAD: &str = "be6c75b85da526349c44e3978374c95e0b80a96d";
const NEXT: &str = "bfe36592eb7f9f2708b5d358b5b5f33ed535c8cf";

fn connected() -> Context {
    Context {
        choices: Choices {
            sonar: Account::server_for_tests("https://sonar.example.com", "profile-1"),
            ..Choices::default()
        },
        ..Context::default()
    }
}

fn tab(ctx: &egui::Context, connection: &Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/quality-gate-test".into(), ctx);
    tab.refs.current = Some("main".into());
    tab.refs.head = HEAD.into();
    Binding::store(
        ctx,
        &tab.path,
        Some(&Binding {
            project: "my:project".into(),
            connection: connection.choices.sonar.identity(),
            ..Binding::default()
        }),
    );
    tab
}

fn passed_answer(connection: &Context) -> Answer {
    Answer {
        query: Query {
            project: "my:project".into(),
            target: Target::Branch("main".into()),
        },
        connection: connection.choices.sonar.identity(),
        result: Ok(Observation {
            target: Target::Branch("main".into()),
            gate: ServerGate::Ok,
            analysis: Some(Analysis {
                key: Some("AX".into()),
                date: None,
                revision: Some(HEAD.into()),
            }),
            pending: false,
            last_task_failed: false,
            failing: Vec::new(),
            dashboard: String::new(),
        }),
        at: Instant::now(),
    }
}

#[test]
fn a_pass_holds_only_for_its_commit_branch_and_connection() {
    let ctx = egui::Context::default();
    let connection = connected();
    connection.run(|| {
        let mut tab = tab(&ctx, &connection);
        assert_eq!(tab.quality_gate_badge(&ctx), Some(Badge::Checking));
        tab.quality_gate.answer = Some(passed_answer(&connection));
        assert_eq!(
            tab.quality_gate_badge(&ctx),
            Some(Badge::Verdict(Verdict::Passed))
        );
        // A new commit: the same answer is stale at once, before any refresh.
        tab.refs.head = NEXT.into();
        assert_eq!(
            tab.quality_gate_badge(&ctx),
            Some(Badge::Verdict(Verdict::Stale(
                Stale::Mismatch {
                    analyzed: HEAD.into(),
                    open: NEXT.into()
                },
                ServerGate::Ok
            )))
        );
        // Another branch: main's answer is not shown for it.
        tab.refs.head = HEAD.into();
        tab.refs.current = Some("feature/x".into());
        assert_eq!(tab.quality_gate_badge(&ctx), Some(Badge::Checking));
        tab.refs.current = None;
        assert_eq!(tab.quality_gate_badge(&ctx), Some(Badge::NoTarget));
    });
    // Another connection: the binding no longer applies.
    let mut other = connected();
    other.choices.sonar.origin = "https://other.example.com".into();
    let tab = connection.run(|| tab(&ctx, &connection));
    other.run(|| {
        assert_eq!(
            tab.quality_gate_badge(&ctx),
            Some(Badge::Failed(Failure::BindingMismatch))
        );
    });
    Context::default().run(|| {
        assert_eq!(tab.quality_gate_badge(&ctx), Some(Badge::NoConnection));
        Binding::store(&ctx, &tab.path, None);
        assert_eq!(tab.quality_gate_badge(&ctx), None);
    });
    connection.run(|| assert_eq!(tab.quality_gate_badge(&ctx), Some(Badge::Unbound)));
}

#[test]
fn reads_start_only_when_due_and_the_network_allows() {
    let ctx = egui::Context::default();
    let connection = connected();
    connection.run(|| {
        let mut tab = tab(&ctx, &connection);
        {
            let _paused = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
            let ((), jobs) =
                crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, false));
            assert!(
                jobs.is_empty(),
                "a metered connection pauses background reads"
            );
            // Asked for explicitly, a metered connection does not refuse it.
            let ((), jobs) =
                crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, true));
            assert_eq!(jobs.len(), 1);
        }
        tab.quality_gate.loading = None;
        let _online = crate::network::test_pause::set(None);
        let ((), jobs) = crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, false));
        assert_eq!(jobs.len(), 1);
        let ((), jobs) = crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, false));
        assert!(jobs.is_empty(), "one read at a time");
        tab.quality_gate.loading = None;
        tab.quality_gate.answer = Some(passed_answer(&connection));
        tab.quality_gate.next = Some(Instant::now() + REFRESH);
        let ((), jobs) = crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, false));
        assert!(jobs.is_empty(), "a fresh answer is not read again");
        {
            let _offline = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
            let ((), jobs) =
                crate::repo::messages::record_jobs(|| tab.poll_quality_gate(&ctx, true));
            assert!(
                jobs.is_empty(),
                "offline mode refuses even an asked refresh"
            );
        }
    });
}
