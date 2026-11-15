//! Assertions that preserve fixture errors under the repository's no-unwrap lint policy.
use std::fmt::Debug;

pub(in crate::app) trait AssertValue {
    type Value;
    fn assert_value(self) -> Self::Value;
}

impl<T, E: Debug> AssertValue for Result<T, E> {
    type Value = T;
    fn assert_value(self) -> T {
        assert!(self.is_ok(), "fixture failed: {:?}", self.as_ref().err());
        match self {
            Ok(value) => value,
            Err(_) => unreachable!(),
        }
    }
}

impl<T> AssertValue for Option<T> {
    type Value = T;
    fn assert_value(self) -> T {
        assert!(self.is_some(), "expected fixture value");
        match self {
            Some(value) => value,
            None => unreachable!(),
        }
    }
}

/// Exclusive ownership prevents retained Windows fixtures from being reused.
pub(in crate::app) fn tree(name: &str) -> crate::repo::hang_diagnostics::FixtureRoot {
    use crate::repo::hang_diagnostics as diag;
    let _stage = diag::stage(format_args!("create fixture {name}"));
    let temp = tempfile::Builder::new()
        .prefix(&format!("gitcraft-library-{name}-"))
        .tempdir()
        .assert_value();
    let path = temp.keep();
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let path = super::super::strip_verbatim(path);
    diag::FixtureRoot(path)
}

/// Drain live repository reads before fixture removal, including during assertion unwinding.
pub(in crate::app) struct LiveHarness(pub egui_kittest::Harness<'static, Option<crate::app::App>>);

impl std::ops::Deref for LiveHarness {
    type Target = egui_kittest::Harness<'static, Option<crate::app::App>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for LiveHarness {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for LiveHarness {
    fn drop(&mut self) {
        use crate::repo::hang_diagnostics as diag;
        use std::time::{Duration, Instant};
        let _stage = diag::stage(format_args!("close library harness"));
        let ctx = self.0.ctx.clone();
        if let Some(app) = self.0.state_mut().as_mut() {
            for tab in std::mem::take(&mut app.tabs) {
                if let crate::app::Tab::Repo(tab) = tab {
                    drop(diag::live_tab(*tab));
                }
            }
            app.library.scheduler.retire_all();
            let until = Instant::now() + Duration::from_secs(10);
            while app.library.scheduler.threads() > 0 || app.library.checks.busy() {
                if Instant::now() >= until {
                    eprintln!("[test-cleanup warning] library scans/checks still running");
                    break;
                }
                app.poll_library(&ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
