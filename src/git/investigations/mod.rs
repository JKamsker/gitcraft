//! Pinned, bounded history investigations. Every request uses the accepted raw transport.
pub(crate) mod views;
use crate::{git, remote};
use std::path::Path;
use std::time::{Duration, Instant};
pub(crate) struct Reader<'a> {
    repo: &'a Path,
    active: &'a dyn Fn() -> bool,
    began: Instant,
}
impl<'a> Reader<'a> {
    pub fn new(repo: &'a Path, active: &'a dyn Fn() -> bool) -> Self {
        Self {
            repo,
            active,
            began: Instant::now(),
        }
    }
    pub fn active(&self) -> bool {
        (self.active)() && self.began.elapsed() < Duration::from_secs(120)
    }
    pub fn run(&self, args: &[&str], input: Vec<u8>, ok: &[i32]) -> Result<Vec<u8>, String> {
        let hooks = format!(
            "core.hooksPath={}",
            if remote::is_remote(self.repo) || !cfg!(windows) {
                "/dev/null"
            } else {
                "NUL"
            }
        );
        let mut pre = vec![
            "--no-lazy-fetch",
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "commit.gpgSign=false",
            "-c",
            &hooks,
        ];
        pre.extend_from_slice(args);
        git::draft_bytes(self.repo, &pre, &[], input, ok, &|| self.active())
            .map_err(|e| e.replace("Draft", "History investigation"))
    }
}
pub(crate) fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
