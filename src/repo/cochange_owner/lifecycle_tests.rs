use super::*;

fn request(effect: Effect) -> Result<Request, String> {
    match effect {
        Effect::Read { request, .. } => Ok(request),
        Effect::None => Err("expected read".into()),
    }
}
fn start(state: &mut State, repo: &str, head: char) -> Result<Request, String> {
    request(state.context(std::path::Path::new(repo), &head.to_string().repeat(40)))
}
fn fail(state: &mut State, request: Request, message: &str) {
    state.complete(Completion {
        request,
        result: Err(message.into()),
    });
}
#[test]
fn a_b_a_rejects_both_old_success_and_error() -> Result<(), String> {
    let mut state = State::default();
    let a = start(&mut state, "/a", 'a')?;
    let b = start(&mut state, "/a", 'b')?;
    let latest = start(&mut state, "/a", 'a')?;
    state.complete(Completion {
        request: a,
        result: Ok(history()),
    });
    fail(&mut state, b, "old failure");
    assert!(state.history.is_none());
    assert!(state.error.is_none());
    fail(&mut state, latest, "current failure");
    assert_eq!(state.error.as_deref(), Some("current failure"));
    state.complete(Completion {
        request: latest,
        result: Ok(history()),
    });
    assert!(
        state.history.is_none(),
        "duplicate completion must not replace failure"
    );
    Ok(())
}
fn history() -> History {
    History {
        records: Vec::new(),
        notes: Vec::new(),
        skipped: 0,
        limited: false,
    }
}
#[test]
fn close_reopen_and_suspend_resume_retire_the_pending_request() -> Result<(), String> {
    let mut state = State::default();
    let old = start(&mut state, "/a", 'a')?;
    state.suspend();
    let new = start(&mut state, "/a", 'a')?;
    assert_ne!(old, new);
    fail(&mut state, old, "late");
    assert!(state.error.is_none());
    state.complete(Completion {
        request: new,
        result: Ok(history()),
    });
    state.suspend();
    assert!(matches!(
        state.context(std::path::Path::new("/a"), &"a".repeat(40)),
        Effect::None
    ));
    assert!(
        state.history.is_some(),
        "completed cache survives suspension"
    );
    Ok(())
}
#[test]
fn repository_change_with_identical_head_and_manual_refresh_are_new_lifetimes() -> Result<(), String>
{
    let mut state = State::default();
    let old = start(&mut state, "/a", 'a')?;
    let new = start(&mut state, "/b", 'a')?;
    fail(&mut state, old, "wrong repository");
    assert!(state.error.is_none());
    state.refresh();
    let refreshed = start(&mut state, "/b", 'a')?;
    fail(&mut state, new, "before refresh");
    assert_eq!(state.requests.pending(), Some(refreshed));
    assert!(state.error.is_none());
    Ok(())
}

#[test]
fn latest_success_clears_the_error_and_late_errors_do_not_erase_the_cache() -> Result<(), String> {
    let mut state = State::default();
    let old = start(&mut state, "/a", 'a')?;
    fail(&mut state, old, "first failure");
    state.refresh();
    let latest = start(&mut state, "/a", 'a')?;
    state.complete(Completion {
        request: latest,
        result: Ok(history()),
    });
    fail(&mut state, old, "late failure");
    assert!(state.error.is_none());
    assert!(state.history.is_some());
    assert!(state.requests.pending().is_none());
    Ok(())
}
