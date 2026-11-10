//! Resolution responsibilities for echo push.
use super::*;

pub(super) fn config(repo: &Reads<'_>, key: &str) -> Result<Vec<String>, String> {
    Ok(repo
        .ok_codes(&["config", "--get-all", key], &[0, 1])?
        .lines()
        .map(str::to_owned)
        .collect())
}
pub(super) fn commit(repo: &Reads<'_>, rev: &str) -> Option<String> {
    repo.run(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{rev}^{{commit}}"),
    ])
    .ok()
    .map(|s| s.trim().into())
}
pub(super) fn branches(repo: &Reads<'_>) -> Result<Vec<String>, String> {
    Ok(repo
        .run(&["for-each-ref", "--format=%(refname)", "refs/heads"])?
        .lines()
        .map(str::to_owned)
        .collect())
}
pub(super) fn default_remote(repo: &Reads<'_>) -> Result<String, String> {
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
pub(super) fn resolved(
    repo: &Reads<'_>,
    steps: &[Vec<String>],
) -> Result<Vec<(String, String)>, String> {
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
                    | "--recurse-submodules=no"
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
pub(super) fn conservative_base(empty_tree: &str) -> String {
    empty_tree.to_owned()
}
pub(super) fn validate_prefix(prefix: &[String]) -> Result<(), String> {
    match prefix {
        [] => Ok(()),
        [flag, value] if flag == "-c" && value.strip_prefix("push.default=").is_some_and(|m| matches!(m, "simple" | "upstream" | "current" | "matching" | "nothing")) => Ok(()),
        _ => Err("Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review".into()),
    }
}
