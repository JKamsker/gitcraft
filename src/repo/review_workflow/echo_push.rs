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
    let pairs = resolved(&reads, steps);
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
            if pairs.is_ok() && push_at(step).is_some() {
                source.bind(&step.iter().map(String::as_str).collect::<Vec<_>>())
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
fn config(repo: &Reads<'_>, key: &str) -> Result<Vec<String>, String> {
    Ok(repo
        .ok_codes(&["config", "--get-all", key], &[0, 1])?
        .lines()
        .map(str::to_owned)
        .collect())
}
fn commit(repo: &Reads<'_>, rev: &str) -> Option<String> {
    repo.run(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{rev}^{{commit}}"),
    ])
    .ok()
    .map(|s| s.trim().into())
}
fn branches(repo: &Reads<'_>) -> Result<Vec<String>, String> {
    Ok(repo
        .run(&["for-each-ref", "--format=%(refname)", "refs/heads"])?
        .lines()
        .map(str::to_owned)
        .collect())
}
fn default_remote(repo: &Reads<'_>) -> Result<String, String> {
    let branch = repo.ok_codes(&["symbolic-ref", "--short", "-q", "HEAD"], &[0, 1])?;
    for key in [
        format!("branch.{}.pushRemote", branch.trim()),
        "remote.pushDefault".into(),
        format!("branch.{}.remote", branch.trim()),
    ] {
        if let Some(value) = config(repo, &key)?.first() {
            return Ok(value.clone());
        }
    }
    let names = repo.run(&["remote"])?;
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
fn resolved(repo: &Reads<'_>, steps: &[Vec<String>]) -> Result<Vec<(String, String)>, String> {
    let empty = repo
        .input(&["hash-object", "-t", "tree", "--stdin"], "")?
        .trim()
        .to_owned();
    let mut pairs = BTreeSet::new();
    for (position, step) in steps.iter().enumerate() {
        let Some(at) = push_at(step) else { continue };
        validate_prefix(&step[..at])?;
        if steps[..position]
            .iter()
            .any(|s| !crate::repo::operations::is_expectation(s))
        {
            return Err("This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation".into());
        }
        let explicit_mode = step[..at].windows(2).find_map(|pair| {
            (pair[0] == "-c")
                .then(|| pair[1].strip_prefix("push.default=").map(str::to_owned))
                .flatten()
        });
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
                repo.run(&["for-each-ref", "--format=%(refname)", "refs/tags"])?
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
                let branch = repo.run(&["symbolic-ref", "--short", "HEAD"])?;
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
                for reference in repo.run(&["for-each-ref", "--format=%(refname)"])?.lines() {
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
                    let full = repo
                        .run(&[
                            "rev-parse",
                            "--symbolic-full-name",
                            "--verify",
                            "--end-of-options",
                            src,
                        ])?
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
        for (src, _dst) in expanded {
            let head = commit(repo, &src)
                .ok_or_else(|| format!("Push source {src} has no inspectable commit tree"))?;
            // No cached fetch ref proves a push-server baseline (split pushurl/custom
            // fetch maps/multiple pushurls). Scan every source tree conservatively.
            let base = conservative_base(&empty);
            pairs.insert((base, head));
        }
    }
    Ok(pairs.into_iter().collect())
}
fn conservative_base(empty_tree: &str) -> String {
    empty_tree.to_owned()
}
fn validate_prefix(prefix: &[String]) -> Result<(), String> {
    match prefix {
        [] => Ok(()),
        [flag, value] if flag == "-c" && value.strip_prefix("push.default=").is_some_and(|m| matches!(m, "simple" | "upstream" | "current" | "matching" | "nothing")) => Ok(()),
        _ => Err("Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review".into()),
    }
}
#[cfg(test)]
fn sources(repo: &Path, steps: &[Vec<String>]) -> Result<Vec<(String, String)>, String> {
    prepare(repo, steps)?.pairs
}
#[cfg(test)]
mod tests {
    use super::*;
    fn steps(args: &[&str]) -> Vec<Vec<String>> {
        vec![args.iter().map(|s| (*s).into()).collect()]
    }
    #[test]
    fn r2_pure_echo_rejects_git_context_prefixes_and_retains_lease_options() {
        for prefix in [
            vec!["--git-dir=/other.git"],
            vec!["-C", "/other"],
            vec!["--namespace=other"],
            vec!["--config-env=remote.origin.url=OTHER"],
            vec!["-c", "core.worktree=/other"],
            vec!["-c", "url.foreign.insteadOf=https://selected"],
            vec!["--unknown-prefix"],
        ] {
            let prefix: Vec<_> = prefix.into_iter().map(str::to_owned).collect();
            assert!(validate_prefix(&prefix).is_err());
        }
        assert!(validate_prefix(&["-c".into(), "push.default=simple".into()]).is_ok());
        // The published source context is prefixed to the original argv: leases, force,
        // upstream behavior and porcelain source identity remain the original operation.
        assert!(contains_push(&steps(&[
            "push",
            "--force-with-lease=refs/heads/main:old",
            "--",
            "origin",
            "HEAD:refs/heads/main"
        ])));
        assert_eq!(conservative_base("empty-tree"), "empty-tree");
    }
    #[test]
    #[ignore = "R2 runtime hold: exact source-after-queued-result and upstream fixture authored, not executed"]
    fn r2_runtime_queued_echo_publishes_only_captured_oid_and_preserves_upstream()
    -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("echo-delayed-source")?;
        let remote = git::stack::test_repo::TestRepo::new("echo-delayed-destination")?;
        r.git(&[
            "remote",
            "add",
            "team",
            remote.path().to_str().ok_or("path")?,
        ])?;
        let d = r.commit("handler.rs", "fn checked() {}\n", "checked D")?;
        let branch = r.git(&["symbolic-ref", "--short", "HEAD"])?;
        let destination = "refs/heads/reviewed";
        let spec = format!("HEAD:{destination}");
        let prepared = prepare(
            r.path(),
            &steps(&["push", "--porcelain", "-u", "team", &spec]),
        )?;
        assert_eq!(prepared.pairs.as_ref().map_err(Clone::clone)?[0].1, d);
        // The preflight result is ready; a terminal advances the original source before
        // the UI callback dispatches the operation. Its private source view remains D.
        let e = r.commit(
            "handler.rs",
            "fn unchecked(){value.unwrap();}\n",
            "unchecked E",
        )?;
        for step in &prepared.steps {
            git::run_unattended(
                Some(r.path()),
                &step.iter().map(String::as_str).collect::<Vec<_>>(),
                None,
            )?;
        }
        assert_eq!(remote.tip(destination)?, d);
        assert_eq!(r.tip("HEAD")?, e);
        assert_eq!(
            r.git(&["config", &format!("branch.{}.remote", branch.trim())])?
                .trim(),
            "team"
        );
        assert_eq!(
            r.git(&["config", &format!("branch.{}.merge", branch.trim())])?
                .trim(),
            destination
        );
        Ok(())
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
            vec![(
                git::run_input(
                    Some(r.path()),
                    &["hash-object", "-t", "tree", "--stdin"],
                    ""
                )?
                .trim()
                .into(),
                head.clone()
            )]
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
        assert_eq!(
            mapped[0].0,
            git::run_input(
                Some(r.path()),
                &["hash-object", "-t", "tree", "--stdin"],
                ""
            )?
            .trim()
        );
        assert_eq!(mapped[0].1, r.tip("refs/heads/feature")?);
        Ok(())
    }
}
