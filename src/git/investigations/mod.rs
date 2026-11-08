//! Pinned, bounded history investigations. Every request uses the accepted raw transport.
#[cfg(test)]
pub(crate) mod tests;
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
        let pre = self.command(args);
        git::draft_bytes(
            self.repo,
            &pre.iter().map(String::as_str).collect::<Vec<_>>(),
            &[],
            input,
            ok,
            &|| self.active(),
        )
        .map_err(|e| match e.as_str() {
            "Draft cancelled" => "History investigation cancelled".into(),
            _ => e,
        })
    }
    /// The same immutable command policy is inspectable without starting Git.
    pub fn command(&self, args: &[&str]) -> Vec<String> {
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
        pre.into_iter().map(str::to_owned).collect()
    }
    pub fn commits(&self, hashes: &[String]) -> Result<Vec<git::Commit>, String> {
        super::commits::investigation_commits_of(self, hashes)
    }
    pub fn graph_page(&self, tips: &[String], limit: usize) -> Result<Vec<git::Commit>, String> {
        if limit > git::LOG_PAGE
            || tips.is_empty()
            || tips.len() > 30000
            || tips.iter().any(|s| !oid(s))
        {
            return Err(crate::i18n::text("backend-invalid-pinned-graph-request-820048").into());
        }
        super::commits::investigation_page(self, tips, Default::default(), 0, limit)
    }
    pub fn diff_files(&self, from: &str, to: &str) -> Result<Vec<git::ChangedFile>, String> {
        super::diff::investigation_diff_files(self, from, to)
    }
}
pub(crate) fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Read a graph page from pinned tips through the same bounded/no-fetch transport.
pub(crate) fn page(
    repo: &Path,
    tips: &[String],
    skip: usize,
    order: git::LogOrder,
    limit: usize,
    active: &dyn Fn() -> bool,
) -> Result<Vec<git::Commit>, String> {
    if tips.is_empty() {
        return Ok(Vec::new());
    }
    if tips.len() > 30000
        || tips.iter().any(|id| !oid(id))
        || limit > git::LOG_PAGE
        || skip > 100000
    {
        return Err(crate::i18n::text("backend-invalid-pinned-graph-page-request-52f635").into());
    }
    super::commits::investigation_page(&Reader::new(repo, active), tips, order, skip, limit)
}

pub(crate) mod dossier;
pub(crate) mod ignore;

pub(crate) mod cochange;

pub(crate) mod symbols;
