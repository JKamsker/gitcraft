use std::time::Instant;

use super::binding::Binding;
use super::schedule::{BACKOFF_BASE, BACKOFF_CAP, REFRESH};
use super::*;
use crate::analysis::Target;
use crate::analysis::gate::ServerGate;
use crate::forge::PullRequest;

fn pr(number: u64, branch: &str, from_fork: bool) -> PullRequest {
    PullRequest {
        number,
        title: String::new(),
        branch: branch.into(),
        base: "main".into(),
        base_oid: String::new(),
        from_fork,
        head_repo: None,
        sha: String::new(),
        author: String::new(),
        url: String::new(),
        draft: false,
    }
}

fn bound(branches: &str) -> Binding {
    Binding {
        project: " my:project ".into(),
        branches: branches.into(),
        ..Binding::default()
    }
}

#[test]
fn the_target_follows_the_branch_its_open_pull_request_and_aliases() {
    let prs = [pr(7, "feature/x", false), pr(8, "feature/fork", true)];
    let b = bound("master=main, release = release/1.x");
    let target = |branch| b.query(branch, &prs).map(|q| q.target);
    assert_eq!(
        target(Some("feature/x")),
        Some(Target::PullRequest("7".into()))
    );
    // A fork's pull request with the same branch name is another repository's branch.
    assert_eq!(
        target(Some("feature/fork")),
        Some(Target::Branch("feature/fork".into()))
    );
    assert_eq!(target(Some("master")), Some(Target::Branch("main".into())));
    assert_eq!(
        target(Some("release")),
        Some(Target::Branch("release/1.x".into()))
    );
    assert_eq!(target(None), None);
    assert_eq!(target(Some("")), None);
    assert_eq!(
        b.query(Some("main"), &prs).map(|q| q.project),
        Some("my:project".to_owned())
    );
    let no_prs = Binding {
        pull_requests: false,
        ..bound("")
    };
    assert_eq!(
        no_prs.query(Some("feature/x"), &prs).map(|q| q.target),
        Some(Target::Branch("feature/x".into()))
    );
}

#[test]
fn bindings_validate_their_key_and_branch_names() {
    assert_eq!(bound("").validate(), Ok(()));
    assert_eq!(bound("a=b,c=d").validate(), Ok(()));
    assert_eq!(
        bound("a").validate(),
        Err("analysis-binding-branches-invalid")
    );
    assert_eq!(
        bound("a=").validate(),
        Err("analysis-binding-branches-invalid")
    );
    let bad = Binding {
        project: "a b".into(),
        ..Binding::default()
    };
    assert_eq!(bad.validate(), Err("analysis-binding-project-invalid"));
    assert!(Binding::default().pull_requests);
}

#[test]
fn bindings_persist_per_repository_and_several_may_share_a_project() {
    let ctx = egui::Context::default();
    let (a, b) = (
        std::path::Path::new("/repo/a"),
        std::path::Path::new("/repo/b"),
    );
    assert_eq!(Binding::load(&ctx, a), None);
    let binding = bound("");
    Binding::store(&ctx, a, Some(&binding));
    Binding::store(&ctx, b, Some(&binding));
    assert_eq!(Binding::load(&ctx, a), Some(binding.clone()));
    assert_eq!(Binding::load(&ctx, b), Some(binding));
    Binding::store(&ctx, a, None);
    assert_eq!(Binding::load(&ctx, a), None);
    assert!(Binding::load(&ctx, b).is_some());
}

fn query(name: &str) -> Query {
    Query {
        project: "p".into(),
        target: Target::Branch(name.into()),
    }
}

fn answer(name: &str, result: Result<Observation, Failure>) -> Answer {
    Answer {
        query: query(name),
        connection: "c".into(),
        head: String::new(),
        result,
        at: Instant::now(),
    }
}

fn observation(pending: bool) -> Observation {
    Observation {
        target: Target::Branch("main".into()),
        gate: ServerGate::Ok,
        analysis: None,
        pending,
        last_task_failed: false,
        failing: Vec::new(),
        dashboard: String::new(),
    }
}

#[test]
fn only_the_awaited_answer_lands_and_only_for_its_own_query_and_connection() {
    let mut state = State {
        loading: Some((2, Instant::now())),
        ..State::default()
    };
    state.arrived(1, answer("main", Ok(observation(false))));
    assert!(state.answer.is_none() && state.loading.is_some());
    state.arrived(2, answer("main", Ok(observation(false))));
    assert!(state.loading.is_none());
    assert!(state.current(&query("main"), "c").is_some());
    assert!(state.current(&query("other"), "c").is_none());
    assert!(
        state
            .current(&query("main"), "another connection")
            .is_none()
    );
}

/// The waits a sequence of answers schedules, in seconds.
fn waits(results: Vec<Result<Observation, Failure>>) -> Vec<u64> {
    let mut schedule = Schedule::default();
    let at = Instant::now();
    results
        .iter()
        .map(|result| {
            schedule.answered(at, result);
            schedule.next.map_or(0, |next| (next - at).as_secs())
        })
        .collect()
}

#[test]
fn failures_and_stuck_pending_analyses_back_off_exponentially_up_to_ten_minutes() {
    let timeouts = vec![Err(Failure::Timeout); 7];
    assert_eq!(waits(timeouts), [30, 60, 120, 240, 480, 600, 600]);
    let pending = vec![Ok(observation(true)); 6];
    assert_eq!(waits(pending), [30, 60, 120, 240, 480, 600]);
    // Settling resets both streaks.
    let mixed = vec![
        Err(Failure::Timeout),
        Err(Failure::Timeout),
        Ok(observation(false)),
        Err(Failure::Timeout),
        Ok(observation(true)),
        Ok(observation(true)),
        Ok(observation(false)),
        Ok(observation(true)),
    ];
    assert_eq!(waits(mixed), [30, 60, 300, 30, 30, 60, 300, 30]);
    assert_eq!(super::schedule::backoff(u32::MAX), BACKOFF_CAP);
    assert_eq!(super::schedule::backoff(0), BACKOFF_BASE);
}

#[test]
fn retry_after_is_honored_within_bounds_and_on_top_of_the_backoff() {
    assert_eq!(waits(vec![Err(Failure::RateLimited(Some(900)))]), [900]);
    assert_eq!(waits(vec![Err(Failure::RateLimited(Some(1)))]), [60]);
    assert_eq!(waits(vec![Err(Failure::RateLimited(Some(99_999)))]), [3600]);
    assert_eq!(waits(vec![Err(Failure::RateLimited(None))]), [30]);
    let repeated = vec![Err(Failure::RateLimited(Some(60))); 6];
    assert_eq!(waits(repeated), [60, 60, 120, 240, 480, 600]);
}

#[test]
fn after_a_failure_nothing_but_an_explicit_refresh_reads_before_the_backoff_ends() {
    let mut schedule = Schedule::default();
    let at = Instant::now();
    assert!(schedule.due(at, false, true), "nothing read yet");
    schedule.answered(at, &Err(Failure::RateLimited(Some(600))));
    let soon = at + Duration::from_secs(5);
    // Another branch, pull request or HEAD (stale) does not bypass Retry-After.
    assert!(!schedule.due(soon, false, true));
    assert!(!schedule.due(soon, false, false));
    assert!(schedule.due(soon, true, false), "the user asked");
    let later = at + Duration::from_secs(601);
    assert!(schedule.due(later, false, false));
    // A settled answer: a stale one is read at once, a fresh one waits for REFRESH.
    schedule.answered(at, &Ok(observation(false)));
    assert!(schedule.due(soon, false, true));
    assert!(!schedule.due(soon, false, false));
    assert!(schedule.due(at + REFRESH, false, false));
}

#[test]
fn an_answer_schedules_its_state() {
    let mut state = State {
        loading: Some((1, Instant::now())),
        ..State::default()
    };
    let a = answer("main", Err(Failure::Http(503)));
    let at = a.at;
    state.arrived(1, a);
    assert_eq!(state.schedule.not_before, Some(at + BACKOFF_BASE));
}
