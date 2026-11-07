//! Running a search, and the people offered for `author:` / `committer:`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::Plan;
use super::args::HashLookup;
use crate::git::END_OPTS;
use crate::git::commits::push_revisions;
use crate::git::{NO_SIGNATURE, UTF8, run as git, search, search_code};

/// The commits `plan` finds, newest first; among those of the revisions `from` when given
/// (as the graph shows them), else of every branch.
pub fn run(repo: &Path, from: Option<&[String]>, plan: &Plan) -> Result<Vec<String>, String> {
    run_with_order(repo, from, crate::git::LogOrder::default(), plan)
}

pub fn run_with_order(repo: &Path, from: Option<&[String]>, order: crate::git::LogOrder, plan: &Plan) -> Result<Vec<String>, String> {
    let (passes, paths, hash) = match plan {
        Plan::Plain { query, code: true } => return search_code(repo, from, order, query),
        Plan::Plain { query, code: false } => return search(repo, from, order, query),
        Plan::Operators {
            passes,
            paths,
            hash,
        } => (passes, paths, hash),
    };
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut hashes = Vec::new();
    for pass in passes {
        let out = git(Some(repo), &log_args(pass, from, paths, order)).map_err(failed)?;
        for hash in out.lines() {
            if seen.insert(hash.to_owned()) {
                hashes.push(hash.to_owned());
            }
        }
    }
    // First, and once, as in a plain search: the commit the text names, if it matches.
    if let Some(named) = hash.as_ref().and_then(|h| named_commit(repo, h, paths)) {
        hashes.retain(|x| *x != named);
        hashes.insert(0, named);
    }
    Ok(hashes)
}

/// The commit `lookup` names, when the filters and `paths` match it.
fn named_commit(repo: &Path, lookup: &HashLookup, paths: &[String]) -> Option<String> {
    let rev = format!("{}^{{commit}}", lookup.prefix);
    let full = git(Some(repo), &["rev-parse", "--verify", "-q", &rev]).ok()?;
    let full = full.trim();
    let mut args = vec!["log", NO_SIGNATURE, UTF8, "--no-walk", "--format=%H"];
    args.extend(lookup.filters.iter().map(String::as_str));
    args.extend([END_OPTS, full, "--"]);
    args.extend(paths.iter().map(String::as_str));
    let out = git(Some(repo), &args).ok()?;
    out.lines().next().map(str::to_owned)
}

/// `git log` with the filters `pass`, over `from`, limited to `paths`.
fn log_args<'a>(
    pass: &'a [String],
    from: Option<&'a [String]>,
    paths: &'a [String],
    order: crate::git::LogOrder,
) -> Vec<&'a str> {
    let mut args = vec!["log", NO_SIGNATURE, UTF8, "--format=%H"];
    args.extend(order.args());
    args.extend(pass.iter().map(String::as_str));
    push_revisions(&mut args, from);
    // `push_revisions` ends revisions it was given with `--`, and none with every branch.
    if from.is_none() {
        args.push("--");
    }
    args.extend(paths.iter().map(String::as_str));
    args
}

/// What git said, as the search shows it: an invalid regex, a revision gone…
fn failed(e: String) -> String {
    let e = e.trim();
    let e = e.strip_prefix("fatal: ").unwrap_or(e);
    if e.is_empty() {
        "git could not search".to_owned()
    } else {
        format!("git could not search: {e}")
    }
}

/// Someone who wrote (or committed) commits, as `.mailmap` names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub commits: usize,
}

/// The authors (or committers) of every branch's commits, those with the most commits
/// first, each under the name and email `.mailmap` gives them.
pub fn people(repo: &Path, committers: bool) -> Result<Vec<Person>, String> {
    let format = if committers {
        "--format=%cN%x00%cE"
    } else {
        "--format=%aN%x00%aE"
    };
    let mut args = vec!["log", NO_SIGNATURE, UTF8, "--use-mailmap", format];
    push_revisions(&mut args, None);
    let out = match git(Some(repo), &args) {
        Ok(out) => out,
        // Nothing committed yet: nobody to offer.
        Err(e) if e.contains("does not have any commits") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(count_people(&out))
}

fn count_people(out: &str) -> Vec<Person> {
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for line in out.lines() {
        if let Some((name, email)) = line.split_once('\0') {
            *counts.entry((name, email)).or_default() += 1;
        }
    }
    let mut people: Vec<Person> = counts
        .into_iter()
        .map(|((name, email), commits)| Person {
            name: name.to_owned(),
            email: email.to_owned(),
            commits,
        })
        .collect();
    people.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.email.cmp(&b.email))
    });
    people
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_with_most_commits_come_first() {
        let out = "Bo\0b@x\nAl\0a@x\nBo\0b@x\nZoë Ünal\0z@x\nbroken line\n";
        let people = count_people(out);
        let names: Vec<(&str, usize)> = people
            .iter()
            .map(|p| (p.name.as_str(), p.commits))
            .collect();
        assert_eq!(names, [("Bo", 2), ("Al", 1), ("Zoë Ünal", 1)]);
    }

    #[test]
    fn git_errors_read_as_the_search_failing() {
        assert_eq!(
            failed("fatal: invalid regex: Unmatched [\n".to_owned()),
            "git could not search: invalid regex: Unmatched ["
        );
        assert_eq!(failed(String::new()), "git could not search");
    }
}
