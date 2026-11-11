//! What kind of file a path is: a test, a snapshot, a CI configuration.

/// The path in lower case with `/` between its parts, its file name, and its folders.
fn parts(path: &str) -> (String, String) {
    let lower = path.replace('\\', "/").to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or_default().to_owned();
    (lower, name)
}

fn in_folder(lower: &str, folders: &[&str]) -> bool {
    lower
        .split('/')
        .rev()
        .skip(1)
        .any(|segment| folders.contains(&segment))
}

/// A file of tests: in a tests folder, or named as tests are in common languages.
pub fn is_test(path: &str) -> bool {
    let (lower, name) = parts(path);
    let stem = name.split('.').next().unwrap_or_default();
    in_folder(
        &lower,
        &["test", "tests", "__tests__", "spec", "specs", "testing"],
    ) || stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.ends_with("_spec")
        || name.contains(".test.")
        || name.contains(".spec.")
        || camel_test(path)
}

/// `FooTest.java`, `FooTests.cs`, `FooSpec.kt`: a test class named in CamelCase.
fn camel_test(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = name.split('.').next().unwrap_or_default();
    ["Test", "Tests", "Spec"]
        .iter()
        .any(|end| stem.len() > end.len() && stem.ends_with(end))
}

/// A snapshot or approval file, which tests compare their output with.
pub fn is_snapshot(path: &str) -> bool {
    let (lower, name) = parts(path);
    in_folder(&lower, &["__snapshots__", "snapshots", "__snapshot__"])
        || name.ends_with(".snap")
        || name.ends_with(".snap.new")
        || name.ends_with(".ambr")
        || name.contains(".approved.")
        || name.ends_with(".verified.txt")
}

/// Which CI system a file configures, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ci {
    /// GitHub Actions and its kin (Forgejo, Gitea): jobs two spaces in under `jobs:`.
    Actions,
    /// GitLab CI: jobs at the top level.
    GitLab,
    /// Any other (CircleCI, Azure, Jenkins, Buildkite, Travis…).
    Other,
}

pub fn ci_kind(path: &str) -> Option<Ci> {
    let (lower, name) = parts(path);
    let yaml = name.ends_with(".yml") || name.ends_with(".yaml");
    let workflows = [
        ".github/workflows/",
        ".forgejo/workflows/",
        ".gitea/workflows/",
    ];
    if yaml && workflows.iter().any(|w| lower.starts_with(w)) {
        return Some(Ci::Actions);
    }
    if name == ".gitlab-ci.yml" || name == ".gitlab-ci.yaml" || lower.starts_with(".gitlab/ci/") {
        return Some(Ci::GitLab);
    }
    let others = [
        ".circleci/",
        ".buildkite/",
        ".woodpecker/",
        ".azure-pipelines/",
    ];
    let named = [
        "jenkinsfile",
        ".travis.yml",
        "azure-pipelines.yml",
        "bitbucket-pipelines.yml",
        ".woodpecker.yml",
        ".drone.yml",
        "appveyor.yml",
    ];
    if others.iter().any(|o| lower.starts_with(o)) || named.contains(&name.as_str()) {
        return Some(Ci::Other);
    }
    None
}
