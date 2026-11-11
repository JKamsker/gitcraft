//! Structural function history. Pinned trees and rename/copy edges narrow similarity matches;
//! ambiguous lineage and inferred splits are exposed instead of silently picking a new symbol.
#[cfg(feature = "symbol-history")]
mod syntax;
#[cfg(not(feature = "symbol-history"))]
#[path = "symbols/disabled.rs"]
mod syntax;
#[cfg(test)]
mod tests;
use super::{Reader, oid};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Symbol {
    pub name: String,
    pub qualified: String,
    pub path: String,
    pub first: usize,
    pub last: usize,
    pub body: String,
    pub blob: String,
    tokens: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub commit: String,
    pub author: String,
    pub time: i64,
    pub subject: String,
    pub event: String,
    pub symbol: Option<Symbol>,
    pub related: Vec<Symbol>,
    pub confidence: Option<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Biography {
    pub tip: String,
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
    pub limited: bool,
}

pub(crate) fn investigate(
    repo: &Path,
    revision: &str,
    path: &str,
    name: &str,
    line: Option<usize>,
    depth: usize,
    active: &dyn Fn() -> bool,
) -> Result<Answer, String> {
    if !cfg!(feature = "symbol-history") {
        return Err(crate::i18n::text(
            "backend-this-build-has-symbol-history-disabled-enable-the-symbol-history-c9b07c",
        )
        .into());
    }
    if !path_ok(path) || !syntax::supported(path) {
        return Err(crate::i18n::text(
            "backend-choose-a-rust-python-javascript-typescript-tsx-go-c-or-c-source-f-3c5a5f",
        )
        .into());
    }
    if !(16..=2048).contains(&depth) {
        return Err(crate::i18n::text(
            "backend-history-depth-must-be-16-2048-first-parent-commits-41a5b6",
        )
        .into());
    }
    if revision.starts_with('-') || revision.chars().any(char::is_control) || revision.len() > 256 {
        return Err(crate::i18n::text("backend-invalid-history-revision-5c3ef6").into());
    }
    let mut trace = Trace {
        reader: Reader::new(repo, active),
        files: BTreeMap::new(),
        changes: BTreeMap::new(),
        bytes: 0,
    };
    let resolved = trace.reader.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        vec![],
        &[0],
    )?;
    let selected = std::str::from_utf8(&resolved)
        .map_err(|_| crate::i18n::text("backend-invalid-revision-3e52aa"))?
        .trim()
        .to_owned();
    if !oid(&selected) {
        return Err(crate::i18n::text("backend-invalid-pinned-revision-254e96").into());
    }
    let head = trace
        .reader
        .run(&["rev-parse", "--verify", "HEAD"], vec![], &[0])?;
    let head = std::str::from_utf8(&head)
        .map_err(|_| crate::i18n::text("backend-invalid-head-e1661b"))?
        .trim()
        .to_owned();
    let mut tip = head.clone();
    let mut ids = trace.reader.run(
        &[
            "rev-list",
            "--first-parent",
            &format!("--max-count={}", depth + 1),
            &tip,
            "--",
        ],
        vec![],
        &[0],
    )?;
    let parse_ids = |bytes: &[u8]| -> Result<Vec<String>, String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| crate::i18n::text("backend-invalid-history-037731"))?;
        text.lines()
            .map(|s| {
                if oid(s) {
                    Ok(s.into())
                } else {
                    Err(crate::i18n::text("backend-invalid-history-id-4032e7").into())
                }
            })
            .collect()
    };
    let mut commits = parse_ids(&ids)?;
    let mut notes = vec![
        crate::i18n::text(
            "backend-an-unambiguous-same-path-qualified-name-identity-survives-body-re-56dc01",
        )
        .into(),
    ];
    if !commits.contains(&selected) {
        tip = selected.clone();
        ids = trace.reader.run(
            &[
                "rev-list",
                "--first-parent",
                &format!("--max-count={}", depth + 1),
                &tip,
                "--",
            ],
            vec![],
            &[0],
        )?;
        commits = parse_ids(&ids)?;
        notes.push(
            crate::i18n::text(
                "backend-selected-revision-is-outside-the-loaded-first-parent-head-path-th-7752ee",
            )
            .into(),
        );
    }
    let limited = commits.len() > depth;
    commits.truncate(depth);
    if limited {
        notes.push(crate::i18n::message(
            "backend-loaded-the-newest-depth-first-parent-commits-increase-history-dep-5bb262",
            &[("depth", (depth).into())],
        ));
    }
    let mut at = commits
        .iter()
        .position(|id| id == &selected)
        .ok_or(crate::i18n::text(
            "backend-selected-revision-exceeds-history-budget-1187be",
        ))?;
    let mut lookup_path = path.to_owned();
    let mut matches = Vec::new();
    while at < commits.len() {
        let symbols = trace.file(&commits[at], &lookup_path)?;
        matches = symbols
            .iter()
            .filter(|s| match line {
                Some(n) if at == commits.iter().position(|id| id == &selected).unwrap_or(0) => {
                    s.first <= n && n <= s.last
                }
                _ => name.is_empty() || s.name == name || s.qualified == name,
            })
            .cloned()
            .collect();
        if !matches.is_empty() {
            break;
        }
        if line.is_some() && name.is_empty() {
            return Ok(Answer::Choices(symbols.as_ref().clone()));
        }
        if at + 1 >= commits.len() {
            break;
        }
        let diff = trace.diff(&commits[at + 1], &commits[at])?;
        if let Some((old, _)) = diff.edges.iter().find(|(_, new)| new == &lookup_path) {
            lookup_path = old.clone();
        }
        at += 1;
    }
    if matches.len() != 1 {
        if matches.is_empty() {
            return Err(crate::i18n::text(
                "backend-no-matching-function-was-found-within-the-local-history-budget-04d06d",
            )
            .into());
        }
        return Ok(Answer::Choices(matches));
    }
    let seed = matches.remove(0);
    let seed_at = at;
    let mut steps = Vec::new();
    let mut current = seed.clone();
    let mut ended = false;
    let mut incomplete = limited;
    while at + 1 < commits.len() {
        let changes = trace.diff(&commits[at + 1], &commits[at])?;
        if !changes.new.contains(&current.path) && !changes.old.contains(&current.path) {
            at += 1;
            continue;
        }
        let candidates = trace.candidates(&commits[at + 1], &current, &changes, false)?;
        let chosen = match choose(&current, &candidates) {
            Ok(c) => c,
            Err(e) => {
                notes.push(e);
                incomplete = true;
                break;
            }
        };
        let Some((previous, confidence)) = chosen else {
            if changes.limited {
                notes.push(crate::i18n::text("backend-a-bulk-unsupported-file-transition-prevents-proving-creation-line-e9e5ff").into());
                incomplete = true;
            } else {
                steps.push(trace.step(
                    &commits[at],
                    crate::i18n::text("backend-created-d70b9e").into(),
                    Some(current.clone()),
                    vec![],
                    None,
                )?);
                ended = true;
            }
            break;
        };
        if previous.body != current.body
            || previous.path != current.path
            || previous.qualified != current.qualified
        {
            let all = trace.candidates(&commits[at], &previous, &changes, true)?;
            let related = splits(&previous, &current, &all);
            let mut what = event(&previous, &current);
            if !related.is_empty() {
                what.push_str(crate::i18n::text("backend-possible-split-5f50f5"));
            }
            steps.push(trace.step(
                &commits[at],
                what,
                Some(current.clone()),
                related,
                Some(confidence),
            )?);
        }
        current = previous;
        at += 1;
    }
    if !ended {
        let parents = trace.reader.run(
            &["show", "-s", "--format=%P", &commits[at], "--"],
            vec![],
            &[0],
        )?;
        let raw = trace
            .reader
            .run(&["cat-file", "commit", &commits[at]], vec![], &[0])?;
        let (boundary, unproven) = boundary_event(&raw, &parents)?;
        steps.push(trace.step(
            &commits[at],
            display_boundary(boundary).into(),
            Some(current),
            vec![],
            None,
        )?);
        if unproven {
            incomplete = true;
            if boundary.contains("shallow") {
                notes.push(crate::i18n::text("backend-the-earlier-commit-is-a-shallow-cut-point-not-the-function-s-prov-9698ad").into());
            }
        }
    }
    steps.reverse();
    current = seed;
    for next in (0..seed_at).rev() {
        let changes = trace.diff(&commits[next + 1], &commits[next])?;
        if !changes.new.contains(&current.path) && !changes.old.contains(&current.path) {
            continue;
        }
        let candidates = trace.candidates(&commits[next], &current, &changes, true)?;
        let chosen = match choose(&current, &candidates) {
            Ok(c) => c,
            Err(e) => {
                notes.push(e);
                incomplete = true;
                break;
            }
        };
        let Some((following, confidence)) = chosen else {
            if changes.limited {
                notes.push(
                    crate::i18n::text(
                        "backend-a-bulk-unsupported-transition-prevents-proving-deletion-16de7f",
                    )
                    .into(),
                );
                incomplete = true;
            } else {
                steps.push(trace.step(
                    &commits[next],
                    crate::i18n::text("backend-deleted-b48ff3").into(),
                    None,
                    vec![],
                    None,
                )?);
            }
            break;
        };
        if following.body != current.body
            || following.path != current.path
            || following.qualified != current.qualified
        {
            let related = splits(&current, &following, &candidates);
            let mut what = event(&current, &following);
            if !related.is_empty() {
                what.push_str(crate::i18n::text("backend-possible-split-5f50f5"));
            }
            steps.push(trace.step(
                &commits[next],
                what,
                Some(following.clone()),
                related,
                Some(confidence),
            )?);
        }
        current = following;
    }
    if steps.is_empty() {
        steps.push(trace.step(
            &commits[seed_at],
            crate::i18n::text("backend-selected-version-f988b6").into(),
            Some(current),
            vec![],
            None,
        )?);
    }
    Ok(Answer::History(Biography {
        tip,
        steps,
        notes,
        limited: incomplete,
    }))
}

#[cfg(test)]
mod backend_fluent_rename_tests;

mod history;
pub(crate) use history::Answer;
use history::Trace;
use history::boundary_event;
use history::choose;
use history::display_boundary;
use history::event;
use history::path_ok;
#[cfg(test)]
use history::rename_path;
use history::splits;
