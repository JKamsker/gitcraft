//! Conservative identity alignment: workflow job IDs plus matrix bindings; never row order.

use std::collections::{BTreeMap, HashMap};

use super::workflow::{self, Workflow};
use crate::ci::timing::{TimedJob, TimedRun, TimedStep};
use crate::insights::Evidence;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    File(String, String),
    Display(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair {
    pub now: Option<usize>,
    pub before: Option<usize>,
    pub evidence: Evidence,
}

fn compatible(now: &TimedRun, before: &TimedRun) -> bool {
    match (now.info.workflow_id, before.info.workflow_id) {
        (Some(a), Some(b)) => a == b,
        (None, None) => !now.info.path.is_empty() && now.info.path == before.info.path,
        _ => false,
    }
}

/// Capture named matrix expressions only when literal boundaries identify one value.
fn bindings(template: &str, name: &str) -> Option<String> {
    let mut template = template;
    let mut remaining = name;
    let mut values = BTreeMap::new();
    while let Some((prefix, after)) = template.split_once("${{") {
        remaining = remaining.strip_prefix(prefix)?;
        let (expression, rest) = after.split_once("}}")?;
        let expression = expression.trim();
        if !expression.starts_with("matrix.") {
            return None;
        }
        let delimiter = rest.split("${{").next()?;
        let (value, tail) = if delimiter.is_empty() {
            if rest.contains("${{") {
                return None;
            }
            (remaining, "")
        } else {
            let (value, tail) = remaining.split_once(delimiter)?;
            if tail.contains(delimiter) {
                return None;
            }
            (value, remaining.get(value.len()..)?)
        };
        if value.is_empty() || values.insert(expression, value).is_some() {
            return None;
        }
        remaining = tail;
        template = rest;
    }
    if remaining != template || values.is_empty() {
        return None;
    }
    Some(format!("{values:?}"))
}

fn key(job: &TimedJob, workflow: Option<&Workflow>) -> Option<(Key, Evidence)> {
    let Some(workflow) = workflow else {
        return Some((Key::Display(job.name.clone()), Evidence::Inferred));
    };
    let (index, evidence) = workflow::match_job(&job.name, workflow)?;
    let file = &workflow.jobs[index];
    let matrix = if file.matrix {
        let shown = file.name.as_deref().unwrap_or(&file.id);
        if shown.contains("${{") {
            bindings(shown, &job.name)?
        } else {
            let suffix = job
                .name
                .strip_prefix(shown)?
                .strip_prefix(" (")?
                .strip_suffix(')')?;
            let values = suffix.split(", ").collect::<Vec<_>>();
            if file.matrix_axes.is_empty() || file.matrix_axes.len() != values.len() {
                return None;
            }
            let bindings: BTreeMap<_, _> = file
                .matrix_axes
                .iter()
                .zip(values)
                .map(|(axis, value)| (format!("matrix.{axis}"), value))
                .collect();
            format!("{bindings:?}")
        }
    } else if file.calls {
        // The called job is part of identity; the caller alone is insufficient.
        job.name.split_once(" / ")?.1.to_owned()
    } else {
        String::new()
    };
    Some((Key::File(file.id.clone(), matrix), evidence))
}

fn unique_pairs<K: Eq + std::hash::Hash>(now: &[Option<K>], before: &[Option<K>]) -> Vec<Pair> {
    fn counts<K: Eq + std::hash::Hash>(keys: &[Option<K>]) -> HashMap<&K, usize> {
        let mut found = HashMap::new();
        for key in keys.iter().flatten() {
            *found.entry(key).or_insert(0) += 1;
        }
        found
    }
    let a = counts(now);
    let b = counts(before);
    let mut taken = vec![false; before.len()];
    let mut pairs: Vec<Pair> = now
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let matched = key
                .as_ref()
                .filter(|key| a.get(key) == Some(&1) && b.get(key) == Some(&1))
                .and_then(|key| before.iter().position(|other| other.as_ref() == Some(key)));
            if let Some(index) = matched {
                taken[index] = true;
            }
            Pair {
                now: Some(i),
                before: matched,
                evidence: Evidence::Unknown,
            }
        })
        .collect();
    pairs.extend(
        taken
            .iter()
            .enumerate()
            .filter(|(_, used)| !**used)
            .map(|(i, _)| Pair {
                now: None,
                before: Some(i),
                evidence: Evidence::Unknown,
            }),
    );
    pairs
}

pub fn jobs(
    now: &TimedRun,
    before: &TimedRun,
    a: Option<&Workflow>,
    b: Option<&Workflow>,
) -> Vec<Pair> {
    let matches = compatible(now, before);
    // When only one workflow revision was read, compare unique display names;
    // do not compare a file key against an unrelated display-key namespace.
    let (a, b) = if a.is_some() && b.is_some() {
        (a, b)
    } else {
        (None, None)
    };
    let a: Vec<_> = now
        .jobs
        .iter()
        .map(|job| matches.then(|| key(job, a)).flatten())
        .collect();
    let b: Vec<_> = before
        .jobs
        .iter()
        .map(|job| matches.then(|| key(job, b)).flatten())
        .collect();
    let mut pairs = unique_pairs(
        &a.iter()
            .map(|key| key.as_ref().map(|(key, _)| key.clone()))
            .collect::<Vec<_>>(),
        &b.iter()
            .map(|key| key.as_ref().map(|(key, _)| key.clone()))
            .collect::<Vec<_>>(),
    );
    for pair in &mut pairs {
        if let (Some(i), Some(j)) = (pair.now, pair.before) {
            pair.evidence = a[i]
                .as_ref()
                .map(|(_, e)| *e)
                .unwrap_or(Evidence::Unknown)
                .max(b[j].as_ref().map(|(_, e)| *e).unwrap_or(Evidence::Unknown));
        }
    }
    pairs
}

pub fn steps(now: &[TimedStep], before: &[TimedStep]) -> Vec<Pair> {
    let keys = |steps: &[TimedStep]| {
        steps
            .iter()
            .map(|step| {
                let duplicate = steps.iter().filter(|other| other.name == step.name).count() > 1;
                (!step.name.is_empty() && (!duplicate || step.number > 0))
                    .then(|| (step.name.clone(), if duplicate { step.number } else { 0 }))
            })
            .collect::<Vec<_>>()
    };
    let mut pairs = unique_pairs(&keys(now), &keys(before));
    for pair in &mut pairs {
        if pair.now.is_some() && pair.before.is_some() {
            pair.evidence = Evidence::Inferred;
        }
    }
    pairs
}

#[cfg(test)]
mod matrix_tests;
#[cfg(test)]
mod tests;
