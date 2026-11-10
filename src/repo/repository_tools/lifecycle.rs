//! Dependency-free completion policy shared by the feature's asynchronous jobs.
//! Tickets are values captured by owned callbacks, never a global callback registry.
#[derive(Clone)]
pub(super) struct Ticket<S> {
    origin: u64,
    scope: S,
}
pub(super) enum Completion<T> {
    Ignore,
    Ready(T),
    Failed(String),
}
impl<S: PartialEq> Ticket<S> {
    pub(super) fn new(origin: u64, scope: S) -> Self {
        Self { origin, scope }
    }
    pub(super) fn settle<T>(
        &self,
        current: u64,
        scope: &S,
        result: Result<T, String>,
    ) -> Completion<T> {
        if self.origin != current || &self.scope != scope {
            return Completion::Ignore;
        }
        match result {
            Ok(value) => Completion::Ready(value),
            Err(error) => Completion::Failed(error),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Teardown(Option<u64>);
impl Teardown {
    pub(super) fn new(origin: Option<u64>) -> Self {
        Self(origin)
    }
    pub(super) fn retires(self, current: u64) -> bool {
        self.0 == Some(current)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_completion_rejects_repository_and_profile_changes() {
        let ticket = Ticket::new(7, ("repo-a", "profile-a"));
        for scope in [("repo-b", "profile-a"), ("repo-a", "profile-b")] {
            assert!(matches!(
                ticket.settle(7, &scope, Ok(42)),
                Completion::Ignore
            ));
            assert!(matches!(
                ticket.settle::<()>(7, &scope, Err("old failure".into())),
                Completion::Ignore
            ));
        }
        assert!(matches!(
            ticket.settle(7, &("repo-a", "profile-a"), Ok(42)),
            Completion::Ready(42)
        ));
    }
    #[test]
    fn superseded_and_closed_requests_cannot_settle_or_retire_a_new_dialog() {
        let ticket = Ticket::new(7, "repo");
        assert!(matches!(
            ticket.settle(8, &"repo", Ok(42)),
            Completion::Ignore
        ));
        assert!(!Teardown::new(Some(7)).retires(8));
        assert!(!Teardown::new(None).retires(7));
        assert!(Teardown::new(Some(7)).retires(7));
    }
    #[test]
    fn current_errors_remain_visible() {
        let ticket = Ticket::new(7, "repo");
        assert!(
            matches!(ticket.settle::<()>(7, &"repo", Err("failure".into())), Completion::Failed(e) if e == "failure")
        );
    }
}
