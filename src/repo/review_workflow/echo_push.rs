//! Resolve push source trees without contacting or changing a remote. Unknown plans require
//! an explicit advisory override; they never acquire an automatic "checked" result.
use crate::git;
use std::{collections::BTreeSet, path::Path};

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
    steps.iter().any(|s| push_at(s).is_some())
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
fn config(repo: &Path, key: &str) -> Result<Vec<String>, String> {
    Ok(
        git::run_ok_codes(Some(repo), &["config", "--get-all", key], &[0, 1])?
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}
fn commit(repo: &Path, rev: &str) -> Option<String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok()
    .map(|s| s.trim().into())
}
fn branches(repo: &Path) -> Result<Vec<String>, String> {
    Ok(git::run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", "refs/heads"],
    )?
    .lines()
    .map(str::to_owned)
    .collect())
}
fn default_remote(repo: &Path) -> Result<String, String> {
    let branch = git::run_ok_codes(
        Some(repo),
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &[0, 1],
    )?;
    for key in [
        format!("branch.{}.pushRemote", branch.trim()),
        "remote.pushDefault".into(),
        format!("branch.{}.remote", branch.trim()),
    ] {
        if let Some(value) = config(repo, &key)?.first() {
            return Ok(value.clone());
        }
    }
    let names = git::run(Some(repo), &["remote"])?;
    if names.lines().any(|n| n == "origin") {
        return Ok("origin".into());
    }
    let names: Vec<_> = names.lines().collect();
    match names.as_slice() {
        [name] => Ok((*name).into()),
        _ => Err("Push destination is not unambiguous; review its source manually".into()),
    }
}
/// Pinned (base, source) pairs for all explicit push refspecs. An absent cached destination
/// uses the empty tree, so initial publication still checks every outgoing added line.
pub(super) fn sources(repo: &Path, steps: &[Vec<String>]) -> Result<Vec<(String, String)>, String> {
    let empty = git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")?
        .trim()
        .to_owned();
    let mut pairs = BTreeSet::new();
    for (position, step) in steps.iter().enumerate() {
        let Some(at) = push_at(step) else { continue };
        if step[..at].iter().any(|s| s == "-C") {
            return Err("Reviewer echo cannot inspect a push using Git -C; review its repository separately".into());
        }
        if steps[..position]
            .iter()
            .any(|s| !crate::repo::operations::is_expectation(s))
        {
            return Err("This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation".into());
        }
        let mut explicit_mode = None;
        for pair in step[..at].windows(2) {
            if pair[0] == "-c" {
                if let Some(mode) = pair[1].strip_prefix("push.default=") {
                    explicit_mode = Some(mode.to_owned());
                } else if pair[1].starts_with("remote.")
                    || pair[1].starts_with("branch.")
                    || pair[1].starts_with("url.")
                {
                    return Err(
                        "Per-command remote or branch overrides require explicit push review"
                            .into(),
                    );
                }
            }
        }
        let mut positional = vec![];
        let mut options = true;
        let mut all = false;
        let mut tags = false;
        let mut deleting = false;
        for word in &step[at + 1..] {
            if options && word == "--" {
                options = false;
                continue;
            }
            if options && word.starts_with('-') {
                match word.as_str() {
                    "--all" => all = true,
                    "--tags" => tags = true,
                    "--delete" | "-d" => deleting = true,
                    "--porcelain"
                    | "-u"
                    | "--set-upstream"
                    | "--no-verify"
                    | "--atomic"
                    | "--progress"
                    | "-q"
                    | "--quiet"
                    | "-v"
                    | "--verbose"
                    | "-f"
                    | "--force"
                    | "--force-if-includes"
                    | "--follow-tags"
                    | "--no-follow-tags"
                    | "--dry-run"
                    | "-n"
                    | "--force-with-lease" => {}
                    w if w.starts_with("--force-with-lease=")
                        || w.starts_with("--push-option=") => {}
                    _ => {
                        return Err(format!(
                            "Reviewer echo cannot infer push option {word}; review this operation explicitly"
                        ));
                    }
                }
            } else {
                positional.push(word.clone());
            }
        }
        if deleting {
            continue;
        }
        let remote = if positional.is_empty() {
            default_remote(repo)?
        } else {
            positional.remove(0)
        };
        let mappings = config(repo, &format!("remote.{remote}.push"))?;
        let explicit_sources = !positional.is_empty() && !all && !tags;
        let mut specs = positional;
        if specs.is_empty() && !all && !tags {
            specs = mappings.clone();
        }
        if all {
            specs.extend(branches(repo)?);
        }
        if tags {
            specs.extend(
                git::run(
                    Some(repo),
                    &["for-each-ref", "--format=%(refname)", "refs/tags"],
                )?
                .lines()
                .map(str::to_owned),
            );
        }
        if specs.is_empty() {
            let mode = explicit_mode
                .or(config(repo, "push.default")?.first().cloned())
                .unwrap_or_else(|| "simple".into());
            if mode == "matching" {
                specs = branches(repo)?;
            } else if matches!(mode.as_str(), "simple" | "upstream" | "current") {
                let branch = git::run(Some(repo), &["symbolic-ref", "--short", "HEAD"])?;
                let upstream_remote = config(repo, &format!("branch.{}.remote", branch.trim()))?;
                let upstream = config(repo, &format!("branch.{}.merge", branch.trim()))?;
                let destination = if mode != "current" && upstream_remote.first() == Some(&remote) {
                    upstream
                        .first()
                        .cloned()
                        .unwrap_or_else(|| format!("refs/heads/{}", branch.trim()))
                } else {
                    format!("refs/heads/{}", branch.trim())
                };
                specs.push(format!("HEAD:{destination}"));
            } else {
                return Err(format!(
                    "Push mode {mode} has no inferable source; review it explicitly"
                ));
            }
        }

        let mut expanded = vec![];
        for spec in specs {
            let spec = spec.strip_prefix('+').unwrap_or(&spec);
            let (src, dst) = spec.split_once(':').unwrap_or((spec, spec));
            if src.is_empty() {
                continue;
            }
            if src.contains('*') {
                let (prefix, suffix) = src.split_once('*').ok_or("Invalid wildcard source")?;
                if suffix.contains('*') || dst.matches('*').count() != 1 {
                    return Err("Unsupported push wildcard; review it explicitly".into());
                }
                for reference in
                    git::run(Some(repo), &["for-each-ref", "--format=%(refname)"])?.lines()
                {
                    if let Some(middle) = reference
                        .strip_prefix(prefix)
                        .and_then(|s| s.strip_suffix(suffix))
                    {
                        expanded.push((reference.to_owned(), dst.replace('*', middle)));
                    }
                }
            } else {
                let mut destinations = vec![];
                if explicit_sources && !spec.contains(':') {
                    let full = git::run(
                        Some(repo),
                        &[
                            "rev-parse",
                            "--symbolic-full-name",
                            "--verify",
                            "--end-of-options",
                            src,
                        ],
                    )?
                    .trim()
                    .to_owned();
                    for mapping in &mappings {
                        let mapping = mapping.strip_prefix('+').unwrap_or(mapping);
                        if let Some((from, to)) = mapping.split_once(':') {
                            if from == src || from == full {
                                destinations.push(to.to_owned());
                            } else if let Some((prefix, suffix)) = from.split_once('*')
                                && to.matches('*').count() == 1
                                && let Some(middle) = full
                                    .strip_prefix(prefix)
                                    .and_then(|s| s.strip_suffix(suffix))
                            {
                                destinations.push(to.replace('*', middle));
                            }
                        }
                    }
                }
                if destinations.is_empty() {
                    destinations.push(dst.into());
                }
                expanded.extend(destinations.into_iter().map(|dst| (src.into(), dst)));
            }
        }
        for (src, dst) in expanded {
            let head = commit(repo, &src)
                .ok_or_else(|| format!("Push source {src} has no inspectable commit tree"))?;
            let base = if src == "HEAD" && dst == "HEAD" {
                commit(repo, "@{upstream}")
            } else if let Some(review_branch) = dst.strip_prefix("refs/for/") {
                commit(
                    repo,
                    &format!(
                        "refs/remotes/{remote}/{}",
                        review_branch.split('%').next().unwrap_or(review_branch)
                    ),
                )
            } else if !dst.starts_with("refs/") || dst.starts_with("refs/heads/") {
                commit(
                    repo,
                    &format!(
                        "refs/remotes/{remote}/{}",
                        dst.strip_prefix("refs/heads/").unwrap_or(&dst)
                    ),
                )
            } else {
                None
            }
            .unwrap_or_else(|| empty.clone());
            pairs.insert((base, head));
        }
    }
    Ok(pairs.into_iter().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn steps(args: &[&str]) -> Vec<Vec<String>> {
        vec![args.iter().map(|s| (*s).into()).collect()]
    }
    #[test]
    fn explicit_force_publish_wildcard_and_default_sources_are_pinned() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("echo-push-sources")?;
        let base = r.tip("HEAD")?;
        r.git(&["remote", "add", "origin", "https://invalid.example/repo"])?;
        r.git(&["update-ref", "refs/remotes/origin/release", &base])?;
        let head = r.commit("source.rs", "bad();\n", "outgoing")?;
        r.git(&["branch", "feature", &head])?;
        assert_eq!(
            sources(
                r.path(),
                &steps(&[
                    "push",
                    "--force-with-lease=refs/heads/release:old",
                    "--",
                    "origin",
                    "refs/heads/feature:refs/heads/release"
                ])
            )?,
            vec![(base, head.clone())]
        );
        let initial = sources(
            r.path(),
            &steps(&[
                "-c",
                "push.default=simple",
                "push",
                "-u",
                "origin",
                "refs/heads/feature",
            ]),
        )?;
        assert_eq!(initial[0].1, head);
        assert!(
            !sources(
                r.path(),
                &steps(&["push", "origin", "refs/heads/*:refs/heads/*"])
            )?
            .is_empty()
        );
        assert!(!sources(r.path(), &steps(&["push"]))?.is_empty());
        assert!(sources(r.path(), &steps(&["push", "--mirror", "origin"])).is_err());
        assert!(sources(r.path(), &steps(&["push", "--delete", "origin", "old"]))?.is_empty());
        let old = stamp(r.path())?;
        r.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://invalid.example/other",
        ])?;
        assert_ne!(stamp(r.path())?, old);
        r.git(&[
            "config",
            "remote.origin.push",
            "refs/heads/feature:refs/heads/release",
        ])?;
        let mapped = sources(r.path(), &steps(&["push", "origin", "refs/heads/feature"]))?;
        assert_eq!(mapped[0].0, r.tip("refs/remotes/origin/release")?);
        assert_eq!(mapped[0].1, r.tip("refs/heads/feature")?);
        Ok(())
    }
}
