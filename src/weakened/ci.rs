//! CI configuration made to check less: steps allowed to fail, jobs or triggers removed,
//! hooks skipped.

use super::patch::{Hunk, Mark};
use super::paths::Ci;
use super::{Flag, Reason, evidence};

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

/// A job (or trigger) whose block a removed line opened: what the evidence calls it.
fn removed_block(kind: Ci, text: &str) -> Option<String> {
    let (indent, key) = block_key(text)?;
    match kind {
        Ci::Actions if indent == 2 && ACTIONS_TRIGGERS.contains(&key) => {
            Some(format!("removes the `{key}` trigger"))
        }
        Ci::Actions if indent == 2 && !ACTIONS_NOT_JOBS.contains(&key) => {
            Some(format!("removes the job `{key}`"))
        }
        Ci::GitLab if indent == 0 && !key.starts_with('.') && !GITLAB_NOT_JOBS.contains(&key) => {
            Some(format!("removes the job `{key}`"))
        }
        _ => None,
    }
}

/// The flags of one hunk of a CI file.
pub fn flags(kind: Ci, index: usize, hunk: &Hunk, added_keys: &[String]) -> Vec<Flag> {
    let mut out = Vec::new();
    for line in &hunk.lines {
        match line.mark {
            Mark::Added => {
                if let Some(what) = lets_fail(&line.text) {
                    out.push(Flag {
                        reason: Reason::CiAllowsFailure,
                        hunk: Some(index),
                        line: Some(line.number),
                        evidence: evidence(line.number, &format!("adds `{what}`"), &line.text),
                    });
                }
            }
            Mark::Removed => {
                let key = block_key(&line.text).map(|(_, k)| k.to_owned());
                // Moved or renamed in place: added back elsewhere in the file.
                if key.as_ref().is_some_and(|k| added_keys.contains(k)) {
                    continue;
                }
                if let Some(what) = removed_block(kind, &line.text) {
                    out.push(Flag {
                        reason: Reason::CiRemovedJob,
                        hunk: Some(index),
                        line: Some(line.number),
                        evidence: evidence(line.number, &what, &line.text),
                    });
                }
            }
            Mark::Context => {}
        }
    }
    out
}

/// The block keys added anywhere in a file, at whatever depth.
pub fn added_keys(hunks: &[Hunk]) -> Vec<String> {
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.mark == Mark::Added)
        .filter_map(|l| block_key(&l.text).map(|(_, k)| k.to_owned()))
        .collect()
}
