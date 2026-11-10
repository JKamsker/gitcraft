//! Each read answered in-process gives git's own answer: run both and compare the bytes.

use super::*;
use crate::git::fixture::Repo;

/// What git itself prints for `args` in `repo` (not through gitcraft, which would answer).
fn git(repo: &Path, args: &[&str]) -> Answer {
    let mut command = std::process::Command::new("git");
    for (name, value) in crate::git::command::GIT_ENV {
        command.env(name, value);
    }
    let out = command
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git runs");
    Answer {
        code: out.status.code().unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

/// The answer made here, which must be git's; `false` when the read is left to git.
fn same(repo: &Path, args: &[&str]) -> bool {
    let Some(ours) = answer(repo, args) else {
        return false;
    };
    let theirs = git(repo, args);
    assert_eq!(
        (ours.code, ours.stdout.as_bstr(), ours.stderr.as_bstr()),
        (
            theirs.code,
            theirs.stdout.as_bstr(),
            theirs.stderr.as_bstr()
        ),
        "git {args:?}"
    );
    true
}

fn answered(repo: &Path, cases: &[&[&str]]) {
    for args in cases {
        assert!(same(repo, args), "not answered: git {args:?}");
    }
}

fn left_to_git(repo: &Path, cases: &[&[&str]]) {
    for args in cases {
        assert!(answer(repo, args).is_none(), "answered: git {args:?}");
    }
}

#[test]
fn config_lookups_are_gits() -> Result<(), String> {
    let repo = Repo::new("answer-config")?;
    let path = repo.path();
    std::fs::write(
        path.join("included.cfg"),
        "[gitcraft]\n\tfromInclude = yes\n[Mixed \"CaSe\"]\n\tKey = included\n",
    )
    .map_err(|e| e.to_string())?;
    let config = path.join(".git/config");
    let mut text = std::fs::read_to_string(&config).map_err(|e| e.to_string())?;
    text.push_str(concat!(
        "[gitcraft]\n\tflag\n\tName = Va\"l\"ue\\tx\n\tempty =\n\tsize = 2k\n",
        "\tfence = one\n\tfence = two\n\tsignOff = off\n",
        "[Mixed \"CaSe\"]\n\tKey = local\n",
        "[remote \"origin\"]\n\turl = https://example.com/r.git\n",
        "[branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n",
        "[credential \"https://dev.example.com\"]\n\tuseHttpPath = true\n",
        "[include]\n\tpath = ../included.cfg\n",
    ));
    std::fs::write(&config, text).map_err(|e| e.to_string())?;
    answered(
        path,
        &[
            &["config", "--get", "gitcraft.name"],
            &["config", "gitcraft.name"],
            &["config", "--get", "GitCraft.NAME"],
            &["config", "--get", "gitcraft.empty"],
            &["config", "--get", "gitcraft.flag"],
            &["config", "--bool", "--get", "gitcraft.flag"],
            &["config", "--type=bool", "--get", "gitcraft.signOff"],
            &["config", "--local", "--bool", "--get", "gitcraft.signOff"],
            &["config", "--type=int", "--get", "gitcraft.size"],
            &["config", "--get", "gitcraft.missing"],
            &["config", "--get", "mixed.CaSe.key"],
            &["config", "--get", "mixed.case.key"],
            &["config", "--get-all", "mixed.CaSe.key"],
            &["config", "--local", "--get-all", "mixed.CaSe.key"],
            &["config", "--local", "--get-all", "gitcraft.fence"],
            &["config", "--get", "gitcraft.fence"],
            &["config", "--get", "gitcraft.fromInclude"],
            &["config", "--local", "--get", "gitcraft.fromInclude"],
            &["config", "--get-regexp", "^gitcraft\\."],
            &["config", "-z", "--get-regexp", "^gitcraft\\."],
            &[
                "config",
                "--null",
                "--get-regexp",
                "^(branch\\..*\\.(remote|merge|pushremote)|remote\\..*\\.gitcraftfork)$",
            ],
            &[
                "config",
                "--local",
                "-z",
                "--get-regexp",
                "^branch\\..*\\.gitcraft-issue",
            ],
            &["config", "--get-regexp", "MIXED"],
            &["config", "--get-regexp", "^credential\\..*\\.usehttppath$"],
            &[
                "config",
                "--type=bool",
                "--get-regexp",
                "^remote\\..*\\.promisor$",
            ],
            &[
                "config",
                "--get",
                "credential.https://dev.example.com.useHttpPath",
            ],
            &["config", "--get", "user.name"],
            &[
                "config",
                "-z",
                "--local",
                "--get-regexp",
                "^user\\.(name|email)$",
            ],
        ],
    );
    left_to_git(
        path,
        &[
            &["config", "gitcraft.name", "value"],
            &["config", "--global", "--get", "user.name"],
            &["config", "--get-regexp", "!x"],
            &["config", "--get-regexp", "\\d"],
            &["config", "--get", "gitcraft.name", "value-pattern"],
            &["config", "--list"],
            &["config", "--type=int", "--get", "gitcraft.flag"],
        ],
    );
    Ok(())
}

#[test]
fn object_and_ref_reads_are_gits() -> Result<(), String> {
    let repo = Repo::new("answer-objects")?;
    let path = repo.path();
    let base = repo.commit(
        "base",
        &[("DCO", Some("d\n")), (".github/dco.yml", Some("y\n"))],
    )?;
    let head = repo.commit(
        "second\n\nwith a body\n",
        &[
            ("d/x", Some("x\n")),
            ("a.b", Some("ab\n")),
            ("a-b", Some("a\n")),
        ],
    )?;
    std::fs::write(path.join("DCO"), "changed\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "DCO"])?;
    repo.git(&["tag", "-a", "v1", "-m", "release", &base])?;
    repo.git(&["tag", "light", &head])?;
    repo.git(&["branch", "side", &base])?;
    repo.git(&["update-ref", "refs/remotes/origin/main", &head])?;
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ])?;
    repo.git(&["update-ref", "refs/gitcraft/agent/one", &head])?;
    repo.git(&["notes", "add", "-m", "a note", &head])?;
    let missing = "1".repeat(head.len());
    for args in [
        &["rev-parse", "--verify", "-q", "HEAD"][..],
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "refs/heads/side^{commit}",
        ],
        &["rev-parse", "--verify", "-q", "refs/heads/nope^{commit}"],
        &["rev-parse", "--verify", "-q", "refs/tags/v1"],
        &["rev-parse", "--verify", "-q", "refs/tags/v1^{}"],
        &["rev-parse", "--verify", "-q", "refs/tags/v1^{tree}"],
        &["rev-parse", "--verify", "-q", &format!("{head}^{{tree}}")],
        &["rev-parse", "--verify", "-q", &missing],
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{missing}^{{commit}}"),
        ],
        &["rev-parse", "--verify", "--end-of-options", ":0:DCO"],
        &["rev-parse", "--verify", "--end-of-options", ":0:nope"],
        &[
            "--no-lazy-fetch",
            "rev-parse",
            "--verify",
            "--end-of-options",
            ":0:d/x",
        ],
        &["rev-parse", "--verify", "--end-of-options", "HEAD:d"],
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            "HEAD:.github/dco.yml",
        ],
        &["rev-parse", "--verify", "refs/remotes/origin/HEAD"],
        &["cat-file", "-s", &head],
        &["cat-file", "-t", "refs/tags/v1"],
        &["cat-file", "commit", &head],
        &["cat-file", "blob", "HEAD:DCO"],
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &["merge-base", "--is-ancestor", &base, &head],
        &[
            "merge-base",
            "--is-ancestor",
            "--end-of-options",
            &head,
            &base,
        ],
        &["merge-base", "--is-ancestor", &head, &head],
        &["notes", "get-ref"],
        &[
            "notes",
            "--ref=refs/notes/commits",
            "list",
            "--end-of-options",
            &head,
        ],
        &["notes", "--ref=commits", "list", &base],
        &["notes", "--ref=refs/notes/other", "list", &base],
        &["bisect", "log"],
        &[
            "ls-tree",
            "--name-only",
            "HEAD",
            "--",
            "DCO",
            "DCO.md",
            ".github/dco.yml",
            "d",
            "a.b",
            "a-b",
        ],
        &[
            "ls-tree",
            "-z",
            "HEAD",
            "--",
            "d",
            ".github/dco.yml",
            "nope",
        ],
        &[
            "ls-tree",
            "-z",
            "HEAD",
            "--",
            ".github/CODEOWNERS",
            "CODEOWNERS",
        ],
        &["for-each-ref", "--format=%(objectname) %(refname)"],
        &[
            "for-each-ref",
            "--format=%(refname)%00%(symref)",
            "refs/remotes/**/HEAD",
        ],
        &["for-each-ref", "--format=%(refname)", "refs/notes/"],
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/tags/v1",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)%1f%(objectname)%1f%(parent)%1f%(creatordate:unix)%1f%(contents)%1e",
            "refs/gitcraft/agent",
        ],
        &[
            "for-each-ref",
            "--format=%(refname) %(objecttype) %(creatordate:unix) %(committerdate:unix)",
            "refs/tags",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)%(objectname)",
            "refs/gitcraft/nothing/",
        ],
        &["for-each-ref", "--format=%%%(refname)%41", "refs/heads/*"],
        &[
            "for-each-ref",
            "--merged=refs/heads/main",
            "--format=%(refname)",
            "refs/heads",
        ],
        &[
            "for-each-ref",
            "--merged=refs/heads/side",
            "--format=%(refname)",
            "refs/heads",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("--contains={base}"),
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("--contains={head}"),
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
        &["rev-list", "--count", "--all"],
        &[
            "rev-list",
            "--count",
            "--exclude=refs/stash",
            "--exclude=refs/gitcraft/*",
            "--exclude=refs/notes/*",
            "--all",
        ],
        &[
            "rev-list",
            "--count",
            "--exclude=refs/heads/*",
            "--exclude=refs/remotes/*",
            "--exclude=refs/tags/*",
            "--exclude=refs/notes/*",
            "--exclude=refs/gitcraft/*",
            "--all",
        ],
    ] {
        assert!(same(path, args), "not answered: git {args:?}");
    }
    repo.git(&["checkout", "-q", "--detach"])?;
    answered(path, &[&["symbolic-ref", "--short", "-q", "HEAD"]]);
    repo.git(&["checkout", "-q", "main"])?;
    // `main` as a tag too: git writes `heads/main`.
    repo.git(&["tag", "main", &base])?;
    left_to_git(
        path,
        &[
            &["symbolic-ref", "--short", "-q", "HEAD"],
            &["rev-parse", "HEAD"],
            &["rev-parse", "--verify", "-q", "main"],
            &["rev-parse", "--verify", "-q", "HEAD~1"],
            &["rev-parse", "--verify", "-q", &head[..7]],
            &["cat-file", "-p", &head],
            &["cat-file", "commit", "refs/tags/v1"],
            &[
                "for-each-ref",
                "--no-merged=refs/heads/main",
                "--format=%(refname)",
                "refs/heads",
            ],
            &["for-each-ref", "--format=%(refname:short)"],
            &["for-each-ref", "--format=%(contents)", "refs/tags/v1"],
            &["ls-tree", "-r", "HEAD"],
            &["rev-list", "--count", "HEAD"],
            &["rev-list", "--count", "--all", "--exclude=refs/stash"],
            &["ls-tree", "HEAD", "--", "d/"],
            &["ls-tree", "HEAD", "--", "*.b"],
        ],
    );
    Ok(())
}

#[test]
fn branches_by_date_and_a_bisect_are_gits() -> Result<(), String> {
    let repo = Repo::new("answer-dates")?;
    let path = repo.path();
    // `main` ends on the last commit, a second after `two`.
    for (i, branch) in ["one", "two", "main"].iter().enumerate() {
        let date = format!("{} +0000", 1_700_000_000 + i * 100);
        let out = std::process::Command::new("git")
            .current_dir(path)
            .env("GIT_COMMITTER_DATE", &date)
            .env("GIT_AUTHOR_DATE", &date)
            .args(["commit", "-q", "--allow-empty", "-m", branch])
            .output()
            .map_err(|e| e.to_string())?;
        assert!(out.status.success());
        if *branch != "main" {
            repo.git(&["branch", branch])?;
        }
    }
    answered(
        path,
        &[
            &[
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname)%(committerdate:unix)",
                "refs/heads",
            ],
            &[
                "for-each-ref",
                "--sort=committerdate",
                "--format=%(refname)",
                "refs/heads",
            ],
        ],
    );
    repo.git(&["bisect", "start", "HEAD", "one"])?;
    answered(path, &[&["bisect", "log"]]);
    repo.git(&["bisect", "reset"])?;
    Ok(())
}

#[test]
fn tree_order_puts_folders_after_names_they_extend() {
    use std::cmp::Ordering::*;
    assert_eq!(tree_order(".github/dco.yml", false, "DCO", false), Less);
    assert_eq!(tree_order("a.b", false, "a/x", false), Less);
    assert_eq!(tree_order("a-b", false, "a", true), Less);
    assert_eq!(tree_order("a", true, "a.b", false), Greater);
    assert_eq!(tree_order("a", false, "a.b", false), Less);
}
