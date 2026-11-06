//! Which issue a branch was made for, kept in the repository's git config beside the branch's
//! other settings (`branch.<name>.gitcraft-issue` and the like): it goes wherever the
//! repository is (on another machine too), every worktree sees it, and git renames or drops it
//! with the branch.

use std::collections::HashMap;
use std::path::Path;

use crate::forge::Issue;
use crate::git;

/// The issue's number.
const NUMBER: &str = "gitcraft-issue";
const TITLE: &str = "gitcraft-issue-title";
const URL: &str = "gitcraft-issue-url";
/// What of its description stays in view (see [`super::criteria::extract`]).
const CRITERIA: &str = "gitcraft-issue-criteria";

/// The issue a branch is for, as it was when work on it started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IssueLink {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub criteria: String,
}

impl IssueLink {
    pub(crate) fn of(issue: &Issue) -> IssueLink {
        IssueLink {
            number: issue.number,
            title: issue.title.trim().to_owned(),
            url: issue.url.clone(),
            criteria: super::criteria::extract(&issue.body),
        }
    }

    /// The title and description a pull request of the branch starts with: the issue's title,
    /// and what closes the issue once it is merged (GitHub, GitLab, Forgejo and Gitea all read
    /// `Closes #N` so).
    pub(crate) fn pull_request_text(&self) -> (String, String) {
        (self.title.clone(), format!("Closes #{}", self.number))
    }
}

/// The steps that write `link` to `branch`'s config.
pub(crate) fn write_ops(branch: &str, link: &IssueLink) -> Vec<Vec<String>> {
    [
        (NUMBER, link.number.to_string()),
        (TITLE, link.title.clone()),
        (URL, link.url.clone()),
        (CRITERIA, link.criteria.clone()),
    ]
    .into_iter()
    .map(|(key, value)| vec!["config".to_owned(), format!("branch.{branch}.{key}"), value])
    .collect()
}

/// The steps that take the link off `branch`: its number first, which is what makes it a link,
/// so that the card goes even if one of the rest was taken out by hand already.
pub(crate) fn unlink_ops(branch: &str) -> Vec<Vec<String>> {
    [NUMBER, TITLE, URL, CRITERIA]
        .into_iter()
        .map(|key| {
            vec![
                "config".to_owned(),
                "--unset".to_owned(),
                format!("branch.{branch}.{key}"),
            ]
        })
        .collect()
}

/// The issues the branches were made for, by branch.
pub(crate) fn read(repo: &Path) -> Result<HashMap<String, IssueLink>, String> {
    // `-z`: the criteria are several lines. Exit code 1: no branch has one, the usual.
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "config",
            "-z",
            "--get-regexp",
            r"^branch\..*\.gitcraft-issue",
        ],
        &[0, 1],
    )?;
    Ok(parse(&out))
}

/// `git config -z --get-regexp`'s records (`key\nvalue\0`), by branch. A branch's name can have
/// dots in it; the setting's name has none, so it is what follows the last one.
fn parse(out: &str) -> HashMap<String, IssueLink> {
    let mut found: HashMap<String, (Option<u64>, IssueLink)> = HashMap::new();
    for record in out.split('\0').filter(|r| !r.is_empty()) {
        let (key, value) = record.split_once('\n').unwrap_or((record, ""));
        let Some((branch, setting)) = key
            .strip_prefix("branch.")
            .and_then(|rest| rest.rsplit_once('.'))
        else {
            continue;
        };
        let (number, link) = found.entry(branch.to_owned()).or_default();
        match setting.to_lowercase().as_str() {
            NUMBER => *number = value.trim().parse().ok(),
            TITLE => link.title = value.to_owned(),
            URL => link.url = value.to_owned(),
            CRITERIA => link.criteria = value.to_owned(),
            _ => {}
        }
    }
    found
        .into_iter()
        .filter_map(|(branch, (number, link))| {
            Some((
                branch,
                IssueLink {
                    number: number?,
                    ..link
                },
            ))
        })
        .collect()
}

/// Where the work on an issue happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// Its branch checked out in the worktree shown.
    Here,
    /// A new worktree, in the folder git of the repository's machine calls so.
    NewWorktree(String),
    /// The worktree its branch is checked out in already: only the link is written.
    Worktree,
}

/// What starting work on an issue does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    pub branch: String,
    /// Where a new branch starts (a full ref or a commit).
    pub from: String,
    /// The branch is there already (work on the issue started before): it is used as it is.
    pub exists: bool,
    pub place: Place,
}

/// The steps that start the work `start` says, `link` written to the branch's config. A new
/// branch doesn't track where it starts from: its first push makes it a branch of its own on the
/// remote, rather than pushing to `main`.
pub(crate) fn start_ops(start: &Start, link: &IssueLink) -> Vec<Vec<String>> {
    let s = |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).collect() };
    let name = start.branch.as_str();
    let mut ops = Vec::new();
    match (&start.place, start.exists) {
        (Place::Here, false) => {
            ops.push(s(&["branch", "--no-track", name, &start.from]));
        }
        (Place::NewWorktree(path), false) => {
            // A worktree whose folder was deleted by hand still holds its branch for git.
            ops.push(s(&["worktree", "prune"]));
            ops.push(s(&[
                "worktree",
                "add",
                "--no-track",
                "-b",
                name,
                "--",
                path,
                &start.from,
            ]));
        }
        (Place::NewWorktree(path), true) => {
            ops.push(s(&["worktree", "prune"]));
            ops.push(s(&["worktree", "add", "--", path, name]));
        }
        (Place::Here | Place::Worktree, _) => {}
    }
    ops.extend(write_ops(name, link));
    if start.place == Place::Here {
        ops.push(s(&["checkout", name, "--"]));
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_links_of_every_branch() {
        let out = "branch.42-fix.gitcraft-issue\n42\0branch.42-fix.gitcraft-issue-title\nFix it\0\
                   branch.42-fix.gitcraft-issue-criteria\n- [ ] a\n- [x] b\0\
                   branch.release.v1.2.gitcraft-issue\n7\0branch.release.v1.2.gitcraft-issue-url\n\
                   https://x/7\0branch.only-title.gitcraft-issue-title\nno number\0\
                   branch.bad.gitcraft-issue\nseven\0branch.empty.gitcraft-issue-title\0";
        let links = parse(out);
        assert_eq!(links.len(), 2, "{links:?}");
        assert_eq!(
            links.get("42-fix"),
            Some(&IssueLink {
                number: 42,
                title: "Fix it".into(),
                url: String::new(),
                criteria: "- [ ] a\n- [x] b".into(),
            })
        );
        let release = links.get("release.v1.2").cloned().unwrap_or_default();
        assert_eq!((release.number, release.url.as_str()), (7, "https://x/7"));
        assert!(parse("").is_empty());
    }

    #[test]
    fn closes_the_issue_from_the_pull_request() {
        let link = IssueLink {
            number: 42,
            title: "Fix login crash".into(),
            ..Default::default()
        };
        assert_eq!(
            link.pull_request_text(),
            ("Fix login crash".to_owned(), "Closes #42".to_owned())
        );
    }

    #[test]
    fn the_steps_of_each_place() {
        let link = IssueLink {
            number: 1,
            ..Default::default()
        };
        let start = |place: Place, exists: bool| Start {
            branch: "1-x".into(),
            from: "refs/remotes/origin/main".into(),
            exists,
            place,
        };
        let firsts = |s: &Start| -> Vec<String> {
            start_ops(s, &link)
                .iter()
                .map(|op| op[..2].join(" "))
                .collect()
        };
        assert_eq!(
            firsts(&start(Place::Here, false)),
            [
                "branch --no-track",
                "config branch.1-x.gitcraft-issue",
                "config branch.1-x.gitcraft-issue-title",
                "config branch.1-x.gitcraft-issue-url",
                "config branch.1-x.gitcraft-issue-criteria",
                "checkout 1-x"
            ]
        );
        assert_eq!(
            firsts(&start(Place::Here, true))[0],
            "config branch.1-x.gitcraft-issue"
        );
        let new_wt = firsts(&start(Place::NewWorktree("/w".into()), false));
        assert_eq!(new_wt[..2], ["worktree prune", "worktree add"]);
        assert!(!new_wt.iter().any(|op| op.starts_with("checkout")));
        assert_eq!(firsts(&start(Place::Worktree, true)).len(), 4);
    }
}
