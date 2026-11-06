//! CI configuration made to check less: steps allowed to fail, jobs or triggers removed,
//! hooks skipped.

use super::patch::{Hunk, Mark};
use super::paths::Ci;
use super::{Flag, Reason};

/// What an added line of CI configuration does to let failures through.
fn lets_fail(text: &str) -> Option<&'static str> {
    let squeezed: String = text
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if squeezed.starts_with('#') {
        return None;
    }
    [
        ("continue-on-error:true", "continue-on-error: true"),
        ("allow_failure:true", "allow_failure: true"),
        ("soft_fail:true", "soft_fail: true"),
        ("if:false", "if: false"),
        ("||true", "|| true"),
    ]
    .into_iter()
    .find(|(m, _)| squeezed.contains(m))
    .map(|(_, shown)| shown)
}

/// The key a YAML line opens a block with (`  build:`), with how far in it is.
fn block_key(text: &str) -> Option<(usize, &str)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    let key = text.trim().strip_suffix(':')?;
    let plain = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
    plain.then_some((indent, key))
}

/// Keys at a job's level in a GitHub workflow that are no jobs.
const ACTIONS_NOT_JOBS: [&str; 6] = [
    "env",
    "permissions",
    "defaults",
    "concurrency",
    "inputs",
    "outputs",
];

/// Events a workflow runs on.
const ACTIONS_TRIGGERS: [&str; 7] = [
    "push",
    "pull_request",
    "pull_request_target",
    "merge_group",
    "workflow_dispatch",
    "schedule",
    "workflow_call",
];

/// Top-level keys of a GitLab pipeline that are no jobs.
const GITLAB_NOT_JOBS: [&str; 11] = [
    "stages",
    "variables",
    "default",
    "include",
    "workflow",
    "image",
    "services",
    "before_script",
    "after_script",
    "cache",
    "spec",
];

/// A job (or trigger) whose block a removed line opened: what the evidence calls it. A
/// workflow's jobs are `job_indent` in (the least any block key of the file is, past none).
fn removed_block(kind: Ci, text: &str, job_indent: usize) -> Option<String> {
    let (indent, key) = block_key(text)?;
    match kind {
        Ci::Actions if indent == job_indent && ACTIONS_TRIGGERS.contains(&key) => {
            Some(format!("the `{key}` trigger"))
        }
        Ci::Actions if indent == job_indent && !ACTIONS_NOT_JOBS.contains(&key) => {
            Some(format!("the job `{key}`"))
        }
        Ci::GitLab if indent == 0 && !key.starts_with('.') && !GITLAB_NOT_JOBS.contains(&key) => {
            Some(format!("the job `{key}`"))
        }
        _ => None,
    }
}

/// How far in a workflow's jobs are: the least indentation of a block key past none, among
/// the lines the diff shows (two spaces when it shows none).
pub fn job_indent(hunks: &[Hunk]) -> usize {
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter_map(|l| block_key(&l.text).map(|(indent, _)| indent))
        .filter(|&indent| indent > 0)
        .min()
        .unwrap_or(2)
}

/// What the rest of the file tells about one of its hunks.
pub struct FileCi<'a> {
    pub kind: Ci,
    /// Block keys added anywhere in the file, with how far in they are.
    pub added_keys: &'a [(usize, String)],
    pub job_indent: usize,
}

/// The flags of one hunk of a CI file.
pub fn flags(file: &FileCi<'_>, index: usize, hunk: &Hunk) -> Vec<Flag> {
    let mut out = Vec::new();
    for line in &hunk.lines {
        match line.mark {
            Mark::Added => {
                if lets_fail(&line.text).is_some() {
                    out.push(Flag::at(Reason::CiAllowsFailure, index, line, None));
                }
            }
            Mark::Removed => {
                // Moved: added back elsewhere in the file, as far in.
                let key = block_key(&line.text).map(|(i, k)| (i, k.to_owned()));
                if key.as_ref().is_some_and(|k| file.added_keys.contains(k)) {
                    continue;
                }
                if let Some(what) = removed_block(file.kind, &line.text, file.job_indent) {
                    out.push(Flag::at(Reason::CiRemovedJob, index, line, Some(what)));
                }
            }
            Mark::Context => {}
        }
    }
    out
}

/// The block keys added anywhere in a file, with how far in they are.
pub fn added_keys(hunks: &[Hunk]) -> Vec<(usize, String)> {
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.mark == Mark::Added)
        .filter_map(|l| block_key(&l.text).map(|(i, k)| (i, k.to_owned())))
        .collect()
}
