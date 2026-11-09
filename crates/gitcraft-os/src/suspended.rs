//! One production ordering for a suspended Windows launch and its recording backend.
use std::io;

pub(super) fn validate_thread_owner(
    expected: u32,
    actual: u32,
    child_active: bool,
) -> io::Result<()> {
    if expected > 1 && actual == expected && child_active {
        Ok(())
    } else {
        Err(io::Error::other(
            "suspended thread no longer belongs to the live owned child",
        ))
    }
}

pub(super) fn wait_empty(
    mut active: impl FnMut() -> io::Result<u32>,
    mut expired: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> io::Result<()> {
    loop {
        if active()? == 0 {
            return Ok(());
        }
        if expired() {
            return Err(io::Error::other(
                "owned Job still has active processes; cleanup unknown",
            ));
        }
        pause();
    }
}

pub(super) trait Backend {
    type Child;
    type Job;
    fn spawn_suspended(&mut self) -> io::Result<Self::Child>;
    fn assign(&mut self, child: &Self::Child) -> io::Result<Self::Job>;
    fn resume(&mut self, child: &Self::Child) -> io::Result<()>;
    fn abort_and_reap(&mut self, child: &mut Self::Child);
}

pub(super) fn launch<B: Backend>(backend: &mut B) -> io::Result<(B::Child, B::Job)> {
    let mut child = backend.spawn_suspended()?;
    let job = match backend.assign(&child) {
        Ok(job) => job,
        Err(error) => {
            backend.abort_and_reap(&mut child);
            return Err(error);
        }
    };
    if let Err(error) = backend.resume(&child) {
        // Closing the already assigned kill-on-close Job precedes final child reaping.
        drop(job);
        backend.abort_and_reap(&mut child);
        return Err(error);
    }
    Ok((child, job))
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    type Events = Rc<RefCell<Vec<&'static str>>>;
    #[test]
    fn receipts_require_empty_job_and_resumed_thread_requires_live_owner_identity() {
        let mut active = [3, 2, 0].into_iter();
        let mut pauses = 0;
        assert!(wait_empty(|| Ok(active.next().unwrap_or(0)), || false, || pauses += 1).is_ok());
        assert_eq!(pauses, 2);
        assert!(wait_empty(|| Ok(1), || true, || panic!("no pause after deadline")).is_err());
        assert!(
            wait_empty(
                || Err(io::Error::other("query failed")),
                || false,
                || panic!("no pause after failure")
            )
            .is_err()
        );
        assert!(validate_thread_owner(42, 42, true).is_ok());
        assert!(validate_thread_owner(42, 43, true).is_err());
        assert!(validate_thread_owner(42, 42, false).is_err());
        assert!(validate_thread_owner(0, 0, true).is_err());
    }
    struct Job(Events);
    impl Drop for Job {
        fn drop(&mut self) {
            self.0.borrow_mut().push("close owned Job");
        }
    }
    struct Recording {
        events: Events,
        fail: Option<&'static str>,
    }
    impl Recording {
        fn event(&self, stage: &'static str) -> io::Result<()> {
            self.events.borrow_mut().push(stage);
            if self.fail == Some(stage) {
                Err(io::Error::other(stage))
            } else {
                Ok(())
            }
        }
    }
    impl Backend for Recording {
        type Child = ();
        type Job = Job;
        fn spawn_suspended(&mut self) -> io::Result<()> {
            self.event("spawn suspended")
        }
        fn assign(&mut self, _: &()) -> io::Result<Job> {
            self.event("assign owned Job")?;
            Ok(Job(self.events.clone()))
        }
        fn resume(&mut self, _: &()) -> io::Result<()> {
            self.event("resume owned child")
        }
        fn abort_and_reap(&mut self, _: &mut ()) {
            self.events.borrow_mut().push("kill and reap owned child");
        }
    }
    #[test]
    fn only_an_assigned_child_resumes_and_all_failures_retire_ownership() {
        for (fail, expected) in [
            (Some("spawn suspended"), vec!["spawn suspended"]),
            (
                Some("assign owned Job"),
                vec![
                    "spawn suspended",
                    "assign owned Job",
                    "kill and reap owned child",
                ],
            ),
            (
                Some("resume owned child"),
                vec![
                    "spawn suspended",
                    "assign owned Job",
                    "resume owned child",
                    "close owned Job",
                    "kill and reap owned child",
                ],
            ),
            (
                None,
                vec![
                    "spawn suspended",
                    "assign owned Job",
                    "resume owned child",
                    "close owned Job",
                ],
            ),
        ] {
            let events = Events::default();
            let result = launch(&mut Recording {
                events: events.clone(),
                fail,
            });
            assert_eq!(result.is_err(), fail.is_some());
            drop(result);
            assert_eq!(*events.borrow(), expected);
        }
    }
}
