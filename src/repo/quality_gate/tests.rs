use std::time::Instant;

use super::binding::Binding;
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

#[test]
fn pending_analyses_poll_sooner_and_rate_limits_wait_as_asked() {
    let wait = |result| {
        let mut state = State {
            loading: Some((1, Instant::now())),
            ..State::default()
        };
        let a = answer("main", result);
        let at = a.at;
        state.arrived(1, a);
        state.next.map(|next| next - at)
    };
    assert_eq!(wait(Ok(observation(true))), Some(PENDING_POLL));
    assert_eq!(wait(Ok(observation(false))), Some(REFRESH));
    assert_eq!(wait(Err(Failure::Timeout)), Some(RETRY));
    assert_eq!(
        wait(Err(Failure::RateLimited(Some(600)))),
        Some(Duration::from_secs(600))
    );
    assert_eq!(
        wait(Err(Failure::RateLimited(Some(1)))),
        Some(Duration::from_secs(60))
    );
}
