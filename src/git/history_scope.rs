//! Presentation-only revision roots shared by history, attribution and backlinks.

use super::{END_OPTS, run};
use std::path::Path;

pub const DEFAULT_HIDDEN_NAMESPACES: &[&str] = &["refs/t3/*"];
const INTERNAL: &[&str] = &[
    "refs/stash",
    "refs/gitcraft/*",
    "refs/notes/*",
    "refs/prefetch/*",
];

/// Patterns are full ref namespace patterns, never revision expressions or command options.
pub fn namespace_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| {
            s.starts_with("refs/")
                && !s.chars().any(char::is_whitespace)
                && !s.contains(['\0', ':', '^', '~', '?', '[', '\\'])
        })
        .map(str::to_owned)
        .collect()
}

pub fn default_hidden_namespaces() -> Vec<String> {
    DEFAULT_HIDDEN_NAMESPACES
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// Git excludes roots, never reachable ancestors of visible branches. HEAD stays a root.
pub(crate) fn revision_args(from: Option<&[String]>, excluded: &[String]) -> Vec<String> {
    match from {
        Some(revs) => std::iter::once(END_OPTS.to_owned())
            .chain(revs.iter().cloned())
            .chain(std::iter::once("--".to_owned()))
            .collect(),
        None => INTERNAL
            .iter()
            .copied()
            .chain(excluded.iter().map(String::as_str))
            .map(|s| format!("--exclude={s}"))
            .chain(std::iter::once("--all".to_owned()))
            .collect(),
    }
}

/// Exact number of commits reachable only from excluded namespaces, across the whole scope.
/// On explicitly selected branches the namespaces aren't roots, so none are excluded.
pub fn excluded_namespace_count(
    repo: &Path,
    from: Option<&[String]>,
    excluded: &[String],
) -> Result<usize, String> {
    if from.is_some() || excluded.is_empty() {
        return Ok(0);
    }
    let count = |patterns: &[String]| -> Result<usize, String> {
        let roots = revision_args(None, patterns);
        let mut args = vec!["rev-list", "--count"];
        args.extend(roots.iter().map(String::as_str));
        run(Some(repo), &args)?
            .trim()
            .parse::<usize>()
            .map_err(|e| e.to_string())
    };
    Ok(count(&[])?.saturating_sub(count(excluded)?))
}

/// Resolve configured roots once for search, retaining its existing safe revision API.
pub fn search_roots(
    repo: &Path,
    from: Option<&[String]>,
    excluded: &[String],
) -> Result<Vec<String>, String> {
    if let Some(from) = from {
        return Ok(from.to_vec());
    }
    let roots = revision_args(None, excluded);
    let mut args = vec!["rev-parse", "--revs-only"];
    args.extend(roots.iter().map(String::as_str));
    Ok(run(Some(repo), &args)?.lines().map(str::to_owned).collect())
}

#[cfg(test)]
#[path = "history_scope_tests.rs"]
mod tests;
