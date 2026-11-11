//! Which issue a branch was made for, kept in the repository's git config beside the branch's
//! other settings (`branch.<name>.gitcraft-issue` and the like): every worktree of the clone
//! sees it, over WSL and SSH too (it's kept where the repository is), and git renames or drops
//! it with the branch. It isn't pushed: another clone of the repository doesn't have it.

use std::collections::HashMap;
use std::path::Path;

use super::criteria::Kept;
use crate::forge::Issue;
use crate::git;
use crate::repo::operations::try_step;

/// The issue's number.
const NUMBER: &str = "gitcraft-issue";
const TITLE: &str = "gitcraft-issue-title";
const URL: &str = "gitcraft-issue-url";
/// What of its description stays in view (see [`super::criteria::extract`]).
const CRITERIA: &str = "gitcraft-issue-criteria";
/// Which part of the description that is ([`Kept::key`]).
const KEPT: &str = "gitcraft-issue-kept";
/// The day the issue was read (`2026-11-06`): what the card shows is the issue as it was then.
const READ: &str = "gitcraft-issue-read";
const TRACKER: &str = "gitcraft-issue-source";

/// The issue a branch is for, as it was when it was last read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IssueLink {
    pub tracker: Option<crate::trackers::Identity>,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub criteria: String,
    pub kept: Kept,
    /// The day it was read, empty for a link written before gitcraft kept it.
    pub read: String,
}

impl IssueLink {
    pub(crate) fn key(&self) -> String {
        self.tracker
            .as_ref()
            .map_or_else(|| format!("#{}", self.number), |i| i.key.clone())
    }
    /// The link to `issue`, read today.
    pub(crate) fn of(issue: &Issue) -> IssueLink {
        let (kept, criteria) = issue.acceptance.as_ref().map_or_else(
            || super::criteria::extract(&issue.body),
            |field| (Kept::Criteria, super::criteria::cap(field.trim())),
        );
        IssueLink {
            tracker: issue.tracker.clone(),
            number: issue.number,
            title: issue.title.trim().to_owned(),
            url: issue.url.clone(),
            criteria,
            kept,
            read: today(),
        }
    }

    /// The title and description a pull request of the branch starts with: the issue's title,
    /// and what closes the issue once it is merged (GitHub, GitLab, Forgejo and Gitea all read
    /// `Closes #N` so).
    pub(crate) fn pull_request_text(&self) -> (String, String) {
        (
            self.title.clone(),
            self.tracker.as_ref().map_or_else(
                || format!("Closes #{}", self.number),
                |i| format!("Relates to {}: {}", i.key, self.url),
            ),
        )
    }
}

/// Today, `2026-11-06`, in UTC: from the clock alone, without a calendar library.
pub(crate) fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil_date((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The year, month and day `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The steps that write `link` to `branch`'s config.
pub(crate) fn write_ops(branch: &str, link: &IssueLink) -> Vec<Vec<String>> {
    [
        (NUMBER, link.number.to_string()),
        (TITLE, link.title.clone()),
        (URL, link.url.clone()),
        (CRITERIA, link.criteria.clone()),
        (KEPT, link.kept.key().to_owned()),
        (READ, link.read.clone()),
        (
            TRACKER,
            link.tracker
                .as_ref()
                .map(|t| serde_json::json!(t).to_string())
                .unwrap_or_default(),
        ),
    ]
    .into_iter()
    .map(|(key, value)| vec!["config".to_owned(), format!("branch.{branch}.{key}"), value])
    .collect()
}

/// Takes the link off `branch`: its number first, which is what makes it a link, so that the
/// card goes even if one of the rest can't be taken out. A setting that isn't there (taken out by
/// hand, or a link from before it was kept) is no error: git says so with exit code 5.
pub(crate) fn unlink(repo: &Path, branch: &str) -> Result<(), String> {
    for key in [NUMBER, TITLE, URL, CRITERIA, KEPT, READ, TRACKER] {
        let name = format!("branch.{branch}.{key}");
        git::run_ok_codes(Some(repo), &["config", "--unset", &name], &[0, 5])?;
    }
    Ok(())
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
            KEPT => link.kept = Kept::from_key(value),
            READ => link.read = value.trim().to_owned(),
            TRACKER => link.tracker = serde_json::from_str(value).ok(),
            _ => {}
        }
    }
    found
        .into_iter()
        .filter_map(|(branch, (number, link))| {
            if number == Some(0) && link.tracker.is_none() {
                return None;
            }
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

/// Where the branch comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// A new branch from `from` (a full ref or a commit), tracking nothing: its first push makes
    /// it a branch of its own on the remote, rather than pushing to `main`.
    New { from: String },
    /// A new branch tracking the remote one `from` (`refs/remotes/origin/42-x`): the work on the
    /// issue started elsewhere, and goes on from there.
    Track { from: String },
    /// The branch is there already (work on the issue started before): it is used as it is.
    Exists,
}

/// What starting work on an issue does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    pub branch: String,
    pub source: Source,
    pub place: Place,
    /// The remote and its branch to fetch first (`origin`, `dev`), so that a new branch starts
    /// from it as the remote has it now. Tried: offline, it starts from it as last fetched.
    pub fetch: Option<(String, String)>,
}

/// The steps that start the work `start` says, `link` written to the branch's config once the
/// branch is there (and checked out, for here): a step that fails leaves no link behind.
pub(crate) fn start_ops(start: &Start, link: &IssueLink) -> Vec<Vec<String>> {
    let s = |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).collect() };
    let name = start.branch.as_str();
    let mut ops = Vec::new();
    // A worktree whose folder was deleted by hand still holds its branch for git: checking it
    // out, here or in a new worktree, would be refused.
    if start.source == Source::Exists || matches!(start.place, Place::NewWorktree(_)) {
        ops.push(s(&["worktree", "prune"]));
    }
    if let Some((remote, base)) = &start.fetch {
        let says = crate::i18n::message(
            "repo-workflows-issue_work-start-ops-4c405286",
            &[
                ("remote", (remote).clone().into()),
                ("base", (base).clone().into()),
            ],
        );
        ops.push(try_step(&["fetch", remote, base], &says));
    }
    let (track, from) = match &start.source {
        Source::New { from } => ("--no-track", Some(from.as_str())),
        Source::Track { from } => ("--track", Some(from.as_str())),
        Source::Exists => ("", None),
    };
    match (&start.place, from) {
        (Place::Here, Some(from)) => {
            // Made and checked out in one: a checkout refused (changes in the way) makes no
            // branch either.
            ops.push(s(&["checkout", "-b", name, track, from, "--"]));
        }
        (Place::Here, None) => ops.push(s(&["checkout", name, "--"])),
        (Place::NewWorktree(path), Some(from)) => {
            ops.push(s(&["worktree", "add", track, "-b", name, "--", path, from]));
        }
        (Place::NewWorktree(path), None) => {
            ops.push(s(&["worktree", "add", "--", path, name]));
        }
        (Place::Worktree, _) => {}
    }
    ops.extend(write_ops(name, link));
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_links_of_every_branch() -> Result<(), String> {
        let out = "branch.42-fix.gitcraft-issue\n42\0branch.42-fix.gitcraft-issue-title\nFix it\0\
                   branch.42-fix.gitcraft-issue-criteria\n- [ ] a\n- [x] b\0\
                   branch.42-fix.gitcraft-issue-kept\ntasks\0\
                   branch.42-fix.gitcraft-issue-read\n2026-11-06\0\
                   branch.release.v1.2.gitcraft-issue\n7\0branch.release.v1.2.gitcraft-issue-url\n\
                   https://x/7\0branch.only-title.gitcraft-issue-title\nno number\0\
                   branch.bad.gitcraft-issue\nseven\0branch.empty.gitcraft-issue-title\0";
        let links = parse(out);
        assert_eq!(links.len(), 2, "{links:?}");
        assert_eq!(
            links.get("42-fix"),
            Some(&IssueLink {
                tracker: None,
                number: 42,
                title: "Fix it".into(),
                url: String::new(),
                criteria: "- [ ] a\n- [x] b".into(),
                kept: Kept::Tasks,
                read: "2026-11-06".into(),
            })
        );
        let release = links.get("release.v1.2").cloned().unwrap_or_default();
        assert_eq!((release.number, release.url.as_str()), (7, "https://x/7"));
        // Written before gitcraft kept which part, and when.
        assert_eq!((release.kept, release.read.as_str()), (Kept::Criteria, ""));
        assert!(parse("").is_empty());
        Ok(())
    }

    #[test]
    fn tracker_link_roundtrips_and_never_closes_a_forge_number() -> Result<(), String> {
        let identity = crate::trackers::Identity {
            kind: crate::trackers::Kind::Jira,
            origin: "https://site.atlassian.net".into(),
            account: "me@example.org".into(),
            viewer: "account-id".into(),
            id: "123".into(),
            key: "APP-42".into(),
        };
        let link = IssueLink {
            tracker: Some(identity),
            number: 0,
            title: "Fix".into(),
            url: "https://site.atlassian.net/browse/APP-42".into(),
            criteria: "- [ ] passes".into(),
            ..Default::default()
        };
        let records = write_ops("app-42-fix", &link)
            .iter()
            .map(|a| format!("{}\n{}\0", a[1], a[2]))
            .collect::<String>();
        assert_eq!(parse(&records).get("app-42-fix"), Some(&link));
        assert!(link.pull_request_text().1.starts_with("Relates to APP-42:"));
        assert!(!link.pull_request_text().1.contains("#0"));
        assert_eq!(
            write_ops(
                "app-42-fix",
                &IssueLink {
                    number: 42,
                    ..Default::default()
                }
            )
            .last()
            .ok_or("missing branch operation")?[2],
            ""
        );
        assert!(
            parse("branch.bad.gitcraft-issue\n0\0branch.bad.gitcraft-issue-source\ninvalid\0")
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn closes_the_issue_from_the_pull_request() -> Result<(), String> {
        let link = IssueLink {
            number: 42,
            title: "Fix login crash".into(),
            ..Default::default()
        };
        assert_eq!(
            link.pull_request_text(),
            ("Fix login crash".to_owned(), "Closes #42".to_owned())
        );
        Ok(())
    }

    #[test]
    fn days_are_dates() -> Result<(), String> {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(59), (1970, 3, 1));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        assert_eq!(civil_date(20_763), (2026, 11, 6));
        assert_eq!(today().len(), 10);
        Ok(())
    }

    #[test]
    fn the_steps_of_each_place() -> Result<(), String> {
        let link = IssueLink {
            number: 1,
            ..Default::default()
        };
        let new = Source::New {
            from: "refs/remotes/origin/main".into(),
        };
        let start = |place: Place, source: Source| Start {
            branch: "1-x".into(),
            source,
            place,
            fetch: None,
        };
        let firsts = |s: &Start| -> Vec<String> {
            start_ops(s, &link)
                .iter()
                .map(|op| op[..2].join(" "))
                .collect()
        };
        assert_eq!(
            firsts(&start(Place::Here, new.clone())),
            [
                "checkout -b",
                "config branch.1-x.gitcraft-issue",
                "config branch.1-x.gitcraft-issue-title",
                "config branch.1-x.gitcraft-issue-url",
                "config branch.1-x.gitcraft-issue-criteria",
                "config branch.1-x.gitcraft-issue-kept",
                "config branch.1-x.gitcraft-issue-read",
                "config branch.1-x.gitcraft-issue-source",
            ]
        );
        assert_eq!(
            firsts(&start(Place::Here, Source::Exists))[..2],
            ["worktree prune", "checkout 1-x"]
        );
        let new_wt = firsts(&start(Place::NewWorktree("/w".into()), new.clone()));
        assert_eq!(new_wt[..2], ["worktree prune", "worktree add"]);
        assert!(!new_wt.iter().any(|op| op.starts_with("checkout")));
        assert_eq!(firsts(&start(Place::Worktree, Source::Exists)).len(), 8);
        // Fetched first, tried.
        let fetched = Start {
            fetch: Some(("origin".into(), "dev".into())),
            ..start(Place::Here, new)
        };
        let ops = start_ops(&fetched, &link);
        assert_eq!(ops[0][2..], ["fetch", "origin", "dev"]);
        assert_eq!(ops[1][..3], ["checkout", "-b", "1-x"]);
        Ok(())
    }
}
