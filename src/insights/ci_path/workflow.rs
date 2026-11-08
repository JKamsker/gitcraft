//! What a workflow file declares of its jobs' dependencies, read from Git at the run's commit:
//! `jobs.<id>.needs` of a GitHub workflow, `needs:` and `stages:` of a `.gitlab-ci.yml`. The
//! forge names jobs otherwise than the file does (a matrix job once per combination, a custom
//! `name:`), so which file job an API job is can only be matched, and is said to be.

use std::path::Path;

use yaml_rust2::{Yaml, YamlLoader};

use crate::insights::Evidence;

/// One job of a workflow file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileJob {
    /// Its key in the file.
    pub id: String,
    /// Its `name:`, which may hold `${{ … }}` expressions (GitHub).
    pub name: Option<String>,
    /// The ids of the jobs it needs.
    pub needs: Vec<String>,
    /// `needs:` was written: without it a GitLab job waits for the stages before its own.
    pub declared: bool,
    /// It runs once per combination of a matrix (or `parallel:` on GitLab).
    pub matrix: bool,
    /// It calls another workflow (`uses:`), whose jobs the forge lists as `caller / callee`.
    pub calls: bool,
    /// Its stage (GitLab).
    pub stage: String,
}

/// The jobs of a workflow file, in its order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workflow {
    pub jobs: Vec<FileJob>,
    /// The order of the stages (GitLab).
    pub stages: Vec<String>,
    /// It includes or extends what isn't in this file, which may add jobs or change their
    /// `needs`.
    pub includes: bool,
}

/// The text of `path` at commit `sha`, read from the repository (nothing is fetched).
pub fn at_revision(repo: &Path, sha: &str, path: &str) -> Result<String, String> {
    if sha.is_empty() || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not a commit id".to_owned());
    }
    if path.is_empty() || path.starts_with(['-', '/']) {
        return Err("not a workflow file".to_owned());
    }
    // A partial clone would fetch a blob it lacks: not for a view that only reads.
    crate::git::run_env(
        Some(repo),
        &["cat-file", "blob", &format!("{sha}:{path}")],
        &[("GIT_NO_LAZY_FETCH", "1")],
    )
    .map_err(|_| format!("{path} isn't in this repository at {}", short(sha)))
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

fn document(text: &str) -> Result<Yaml, String> {
    let docs = YamlLoader::load_from_str(text).map_err(|e| format!("not YAML: {e}"))?;
    docs.into_iter()
        .next()
        .ok_or_else(|| "an empty file".to_owned())
}

/// A string, or a list of strings (`needs: build`, `needs: [a, b]`); GitLab's `- job: a`
/// entries too.
fn names(value: &Yaml) -> Vec<String> {
    match value {
        Yaml::String(s) => vec![s.clone()],
        Yaml::Array(list) => list
            .iter()
            .filter_map(|item| match item {
                Yaml::String(s) => Some(s.clone()),
                Yaml::Hash(_) => item["job"].as_str().map(str::to_owned),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The jobs of a GitHub workflow file.
pub fn parse_github(text: &str) -> Result<Workflow, String> {
    let doc = document(text)?;
    let jobs = doc["jobs"]
        .as_hash()
        .ok_or("the workflow file has no jobs")?;
    let jobs = jobs
        .iter()
        .filter_map(|(key, job)| {
            let id = key.as_str()?.to_owned();
            Some(FileJob {
                id,
                name: job["name"].as_str().map(str::to_owned),
                needs: names(&job["needs"]),
                declared: !job["needs"].is_badvalue(),
                matrix: !job["strategy"]["matrix"].is_badvalue(),
                calls: job["uses"].as_str().is_some(),
                stage: String::new(),
            })
        })
        .collect();
    Ok(Workflow {
        jobs,
        stages: Vec::new(),
        includes: false,
    })
}

/// Top-level keys of a `.gitlab-ci.yml` that aren't jobs.
const GITLAB_KEYWORDS: [&str; 12] = [
    "stages",
    "variables",
    "include",
    "default",
    "workflow",
    "image",
    "services",
    "cache",
    "before_script",
    "after_script",
    "types",
    "spec",
];

/// The jobs and stages of a `.gitlab-ci.yml`.
pub fn parse_gitlab(text: &str) -> Result<Workflow, String> {
    let doc = document(text)?;
    let top = doc.as_hash().ok_or("the pipeline file isn't a mapping")?;
    let mut includes = !doc["include"].is_badvalue();
    let mut jobs = Vec::new();
    for (key, job) in top {
        let Some(id) = key.as_str() else { continue };
        if id.starts_with('.') || GITLAB_KEYWORDS.contains(&id) || job.as_hash().is_none() {
            continue;
        }
        includes |= !job["extends"].is_badvalue();
        jobs.push(FileJob {
            id: id.to_owned(),
            name: None,
            needs: names(&job["needs"]),
            declared: !job["needs"].is_badvalue(),
            matrix: !job["parallel"].is_badvalue(),
            calls: !job["trigger"].is_badvalue(),
            stage: job["stage"].as_str().unwrap_or("test").to_owned(),
        });
    }
    let stages = match doc["stages"].as_vec() {
        Some(list) => list
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect(),
        None => ["build", "test", "deploy"].map(str::to_owned).to_vec(),
    };
    let mut all = vec![".pre".to_owned()];
    all.extend(stages);
    all.push(".post".to_owned());
    Ok(Workflow {
        jobs,
        stages: all,
        includes,
    })
}

/// Whether `text` fits `template`, a `name:` with `${{ … }}` expressions, whatever they
/// stood for. A template that is nothing but expressions fits nothing: there is nothing to
/// tell by.
fn fits_template(template: &str, text: &str) -> bool {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find("${{") {
        pieces.push(&rest[..open]);
        rest = match rest[open..].find("}}") {
            Some(close) => &rest[open + close + 2..],
            None => "",
        };
    }
    pieces.push(rest);
    if pieces.iter().all(|p| p.trim().is_empty()) {
        return false;
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if pieces.len() == 1 {
        return text == first;
    }
    if !text.starts_with(first) || !text.ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut middle = &text[first.len()..text.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        match middle.find(piece) {
            Some(at) => middle = &middle[at + piece.len()..],
            None => return false,
        }
    }
    true
}

/// Which job of `workflow` the forge's job `name` is, and how sure that is: observed when
/// the names are the same and nothing renames it, inferred for a matrix, a custom name or a
/// called workflow. `None` when no job fits, or more than one fits equally.
pub fn match_job(name: &str, workflow: &Workflow) -> Option<(usize, Evidence)> {
    let mut exact = Vec::new();
    let mut close = Vec::new();
    for (i, job) in workflow.jobs.iter().enumerate() {
        let plain = job.name.is_none() && !job.matrix && !job.calls;
        if plain && name == job.id {
            exact.push(i);
            continue;
        }
        let shown = match &job.name {
            Some(n) if !n.contains("${{") => n.as_str(),
            Some(_) => "",
            None => job.id.as_str(),
        };
        let templated = job
            .name
            .as_deref()
            .is_some_and(|n| n.contains("${{") && fits_template(n, name));
        let fits = templated
            || (!shown.is_empty() && name == shown)
            || (!shown.is_empty()
                && job.matrix
                && name.strip_prefix(shown).is_some_and(matrix_suffix))
            || (!shown.is_empty()
                && job.calls
                && name
                    .strip_prefix(shown)
                    .is_some_and(|rest| rest.starts_with(" / ")));
        if fits {
            close.push(i);
        }
    }
    match (exact.as_slice(), close.as_slice()) {
        ([one], _) => Some((*one, Evidence::Observed)),
        ([], [one]) => Some((*one, Evidence::Inferred)),
        _ => None,
    }
}

/// What the forge adds to a matrix job's name: ` (a, b)` (GitHub), ` 1/3` or `: [a, b]`
/// (GitLab).
fn matrix_suffix(rest: &str) -> bool {
    (rest.starts_with(" (") && rest.ends_with(')'))
        || (rest.starts_with(": [") && rest.ends_with(']'))
        || rest.strip_prefix(' ').is_some_and(|n| {
            n.split_once('/').is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && a.bytes().all(|c| c.is_ascii_digit())
                    && b.bytes().all(|c| c.is_ascii_digit())
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB: &str = r#"
name: CI
on: [push]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps: [{run: cargo clippy}]
  test:
    needs: lint
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps: [{run: cargo test}]
  package:
    name: Package for ${{ matrix.target }}
    needs: [lint, test]
    strategy: {matrix: {target: [x86, arm]}}
    runs-on: ubuntu-latest
  release:
    name: Release
    needs: package
    uses: ./.github/workflows/release.yml
"#;

    #[test]
    fn reads_the_needs_of_github_jobs() {
        let w = parse_github(GITHUB).unwrap_or_default();
        let ids: Vec<&str> = w.jobs.iter().map(|j| j.id.as_str()).collect();
        assert_eq!(ids, ["lint", "test", "package", "release"]);
        assert_eq!(w.jobs[0].needs, Vec::<String>::new());
        assert!(!w.jobs[0].declared);
        assert_eq!(w.jobs[1].needs, ["lint"]);
        assert!(w.jobs[1].matrix);
        assert_eq!(w.jobs[2].needs, ["lint", "test"]);
        assert!(w.jobs[3].calls);
        assert!(parse_github("jobs: 3").is_err());
        assert!(parse_github(": : :\n  - [").is_err());
    }

    #[test]
    fn api_jobs_are_matched_to_file_jobs_and_say_how() {
        let w = parse_github(GITHUB).unwrap_or_default();
        assert_eq!(match_job("lint", &w), Some((0, Evidence::Observed)));
        assert_eq!(
            match_job("test (windows-latest)", &w),
            Some((1, Evidence::Inferred))
        );
        assert_eq!(
            match_job("Package for arm", &w),
            Some((2, Evidence::Inferred))
        );
        assert_eq!(
            match_job("Release / publish", &w),
            Some((3, Evidence::Inferred))
        );
        assert_eq!(match_job("deploy", &w), None);
        // Nothing but an expression: nothing to tell it by.
        assert!(!fits_template("${{ matrix.name }}", "x"));
        assert!(fits_template("a ${{ x }} b ${{ y }} c", "a 1 b 2 c"));
        assert!(!fits_template("a ${{ x }} b", "a 1 c"));
    }

    #[test]
    fn two_jobs_that_fit_equally_match_neither() {
        let w = parse_github(
            "jobs:\n  a:\n    name: Build\n    runs-on: x\n  b:\n    name: Build\n    runs-on: x\n",
        )
        .unwrap_or_default();
        assert_eq!(match_job("Build", &w), None);
    }

    #[test]
    fn reads_gitlab_stages_and_needs() {
        let text = r#"
stages: [build, test, deploy]
include: [{local: ci/extra.yml}]
variables: {A: b}
.template: {script: [x]}
compile:
  stage: build
  script: [make]
rspec:
  stage: test
  parallel: 3
  script: [rspec]
lint:
  stage: test
  needs: []
  script: [lint]
pages:
  stage: deploy
  needs: [{job: rspec}, compile]
  script: [deploy]
"#;
        let w = parse_gitlab(text).unwrap_or_default();
        let ids: Vec<&str> = w.jobs.iter().map(|j| j.id.as_str()).collect();
        assert_eq!(ids, ["compile", "rspec", "lint", "pages"]);
        assert_eq!(w.stages, [".pre", "build", "test", "deploy", ".post"]);
        assert!(w.includes);
        assert!(!w.jobs[0].declared);
        assert!(w.jobs[2].declared && w.jobs[2].needs.is_empty());
        assert_eq!(w.jobs[3].needs, ["rspec", "compile"]);
        assert_eq!(match_job("compile", &w), Some((0, Evidence::Observed)));
        assert_eq!(match_job("rspec 2/3", &w), Some((1, Evidence::Inferred)));
        assert_eq!(match_job("rspec 2/x", &w), None);
    }
}
