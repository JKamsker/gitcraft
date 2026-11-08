//! A bounded queue survives URLs arriving before the application installs its repaint hook.
//! The macOS adapter only receives Apple events; resolution stays in the application's
//! existing command/link resolver and workers.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Wake = Arc<dyn Fn() + Send + Sync>;
const MAX_URL: usize = 4096;
const CAPACITY: usize = 64;

#[derive(Default)]
struct UrlQueue {
    pending: VecDeque<Result<String, String>>,
    dropped: usize,
    wake: Option<Wake>,
}

impl UrlQueue {
    fn push(&mut self, url: Result<String, String>) -> Option<Wake> {
        let url = url.and_then(|url| {
            if url.len() > MAX_URL {
                Err("The gitcraft link is too long (maximum 4096 bytes)".into())
            } else if !url
                .get(..11)
                .is_some_and(|s| s.eq_ignore_ascii_case("gitcraft://"))
            {
                Err("The native URL event did not contain a gitcraft:// link".into())
            } else {
                Ok(url)
            }
        });
        if !self.pending.contains(&url) {
            if self.pending.len() < CAPACITY {
                self.pending.push_back(url);
            } else {
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        self.wake.clone()
    }

    fn take(&mut self) -> Vec<Result<String, String>> {
        let mut urls: Vec<_> = self.pending.drain(..).collect();
        if self.dropped != 0 {
            urls.push(Err(format!(
                "{} native links exceeded the pending-link limit; open them again",
                std::mem::take(&mut self.dropped)
            )));
        }
        urls
    }

    fn attach(&mut self, wake: Wake) -> Option<Wake> {
        self.wake = Some(wake.clone());
        (!self.pending.is_empty() || self.dropped != 0).then_some(wake)
    }
}

static URLS: Mutex<UrlQueue> = Mutex::new(UrlQueue {
    pending: VecDeque::new(),
    dropped: 0,
    wake: None,
});

/// Main-window repaint, attached after the event handler and event loop are ready.
pub fn set_native_url_wake(wake: impl Fn() + Send + Sync + 'static) {
    let wake = URLS.lock().ok().and_then(|mut q| q.attach(Arc::new(wake)));
    if let Some(wake) = wake {
        wake();
    }
}

/// Native URLs in delivery order; duplicates are coalesced only while pending.
pub fn take_native_urls() -> Vec<Result<String, String>> {
    URLS.lock().map(|mut q| q.take()).unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub(super) fn receive(url: Result<String, String>) {
    let wake = URLS.lock().ok().and_then(|mut q| q.push(url));
    if let Some(wake) = wake {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cold_urls_wait_for_consumer_warm_urls_wake_and_pending_duplicates_coalesce() {
        let mut q = UrlQueue::default();
        let first = "gitcraft://open?url=https%3A%2F%2Fgithub.com%2Fo%2Fr%2Fpull%2F12";
        assert!(q.push(Ok(first.into())).is_none());
        q.push(Ok(first.into()));
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let wake: Wake = Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        if let Some(wake) = q.attach(wake) {
            wake();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(q.take(), [Ok(first.into())]);
        for url in [
            first,
            "gitcraft://open?url=https://codeberg.org/o/r/commit/abc",
            "gitcraft://open?url=https://github.com/o/r/blob/main/a.rs%23L40",
        ] {
            if let Some(wake) = q.push(Ok(url.into())) {
                wake();
            }
        }
        assert_eq!(calls.load(Ordering::Relaxed), 4);
        assert_eq!(q.take().len(), 3);
        assert!(q.take().is_empty());
    }

    #[test]
    fn queue_rejects_wrong_schemes_bounds_payload_and_reports_overflow() {
        let mut q = UrlQueue::default();
        q.push(Ok("file:///tmp/repo".into()));
        q.push(Ok("x".repeat(MAX_URL + 1)));
        let errors = q.take();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(Result::is_err));
        for i in 0..CAPACITY + 3 {
            q.push(Ok(format!("gitcraft://open?url={i}")));
        }
        assert_eq!(q.pending.len(), CAPACITY);
        let events = q.take();
        assert_eq!(events.len(), CAPACITY + 1);
        assert!(
            events
                .last()
                .is_some_and(|e| e.as_ref().is_err_and(|e| e.starts_with("3 native links")))
        );
        assert_eq!(q.dropped, 0);
    }
}
