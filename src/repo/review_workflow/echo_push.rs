//! Resolve push source trees without contacting or changing a remote. Unknown plans require
//! an explicit advisory override; they never acquire an automatic "checked" result.
use crate::git;
use std::{collections::BTreeSet, path::Path, sync::Arc};
struct Reads<'a> {
    repo: &'a Path,
    source: &'a git::CommandSource,
}
impl Reads<'_> {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let args = self.source.bind(args);
        git::run(
            Some(self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }
    fn ok_codes(&self, args: &[&str], codes: &[i32]) -> Result<String, String> {
        let args = self.source.bind(args);
        git::run_ok_codes(
            Some(self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            codes,
        )
    }
    fn input(&self, args: &[&str], input: &str) -> Result<String, String> {
        let args = self.source.bind(args);
        git::run_input(
            Some(self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            input,
        )
    }
}
pub(super) struct Prepared {
    pub source: Arc<git::CommandSource>,
    pub steps: Vec<Vec<String>>,
    pub pairs: Result<Vec<(String, String)>, String>,
}
pub(super) fn prepare(repo: &Path, steps: &[Vec<String>]) -> Result<Prepared, String> {
    let source = git::CommandSource::capture(repo)?;
    let reads = Reads {
        repo,
        source: &source,
    };
    let evidence = evidence_steps(steps, |id| {
        let plan = crate::push_preflight::registered(id)?;
        crate::push_preflight::push_args(&plan)
    })?;
    let pairs = resolved(&reads, &evidence);
    if let Ok(pairs) = &pairs {
        for (_, oid) in pairs {
            git::run(
                Some(repo),
                &[
                    "update-ref",
                    &format!("refs/gitcraft/reviewer-echo-inputs/{oid}"),
                    oid,
                ],
            )?;
        }
    }
    let steps = steps
        .iter()
        .map(|step| {
            if pairs.is_ok() && contains_push(std::slice::from_ref(step)) {
                bind_step(step, source.bind(&[]))
            } else {
                step.clone()
            }
        })
        .collect();
    Ok(Prepared {
        source,
        steps,
        pairs,
    })
}

/// Review the original plan's exact OID, while preserving its opaque authorization for
/// dispatch. The private source marker is evidence, never a replacement push operation.
fn evidence_steps(
    steps: &[Vec<String>],
    mut approved: impl FnMut(&str) -> Result<Vec<String>, String>,
) -> Result<Vec<Vec<String>>, String> {
    steps.iter().map(|step| {
        if step.first().is_some_and(|s| s == crate::push_preflight::APPROVED) {
            match step.as_slice() {
                [_, id] => approved(id),
                _ => Err("Reviewed push already has execution evidence; resume its retained operation".into()),
            }
        } else {
            Ok(step.clone())
        }
    }).collect()
}
fn bind_step(step: &[String], marker: Vec<String>) -> Vec<String> {
    if step
        .first()
        .is_some_and(|s| s == crate::push_preflight::APPROVED)
    {
        [step.to_vec(), marker].concat()
    } else {
        [marker, step.to_vec()].concat()
    }
}

fn push_at(step: &[String]) -> Option<usize> {
    let mut i = 0;
    while i < step.len() {
        match step[i].as_str() {
            "-c" | "-C" => i += 2,
            "push" => return Some(i),
            w if w.starts_with('-') => i += 1,
            _ => return None,
        }
    }
    None
}
pub(super) fn contains_push(steps: &[Vec<String>]) -> bool {
    steps.iter().any(|s| {
        push_at(s).is_some()
            || s.first()
                .is_some_and(|s| s == crate::push_preflight::APPROVED)
    })
}
pub(super) fn stamp(repo: &Path) -> Result<String, String> {
    let refs = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
            "refs/replace",
        ],
    )?;
    let head = git::run(Some(repo), &["rev-parse", "HEAD"])?;
    let branch = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    // Hash the configuration; it can contain private remote URLs and must not be displayed.
    let config = git::run(Some(repo), &["config", "--null", "--list"])?;
    Ok(format!(
        "{:x}",
        md5::compute(format!("{refs}\0{head}\0{branch}\0{config}"))
    ))
}
#[cfg(test)]
fn sources(repo: &Path, steps: &[Vec<String>]) -> Result<Vec<(String, String)>, String> {
    prepare(repo, steps)?.pairs
}
#[cfg(test)]
#[path = "echo_push/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "echo_push/integration_tests.rs"]
mod integration_tests;

mod resolution;
use resolution::resolved;
#[cfg(test)]
use resolution::{conservative_base, validate_prefix};
