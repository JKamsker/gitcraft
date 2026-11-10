//! Each read answered in-process gives git's own answer: run both and compare the bytes.

use super::*;
use crate::git::fixture::Repo;

const STATUS: &[&str] = &["status", "--porcelain=v1", "-z", "--untracked-files=all"];

/// Check native blob reads against Git for loose and packed objects and size limits.
#[test]
fn bounded_raw_blobs_match_git_before_and_after_repacking() -> Result<(), String> {
    let repo = Repo::new("bounded-raw-blobs")?;
    repo.commit("base", &[("space ü.cs", Some("old\r\n"))])?;
    std::fs::write(repo.path().join("space ü.cs"), b"new\0\xff\r\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "."])?;
    for packed in [false, true] {
        if packed {
            repo.git(&["repack", "-ad"])?;
        }
        for spec in ["HEAD:space ü.cs", ":0:space ü.cs"] {
            let id = git(repo.path(), &["rev-parse", "--verify", spec]);
            assert_eq!(id.code, 0);
            let oid = std::str::from_utf8(&id.stdout)
                .map_err(|e| e.to_string())?
                .trim();
            let expected = git(repo.path(), &["cat-file", "blob", oid]).stdout;
            assert_eq!(
                bounded_blob(repo.path(), spec, expected.len()),
                Some(Ok(expected.clone()))
            );
            assert_eq!(
                bounded_blob(repo.path(), spec, expected.len() - 1),
                Some(Err(()))
            );
        }
    }
    assert!(bounded_blob(repo.path(), "HEAD:missing", 1024).is_none());
    assert!(bounded_blob(repo.path(), "HEAD", 1024).is_none());
    Ok(())
}

/// Reopening the repository must observe index and packed-refs changes even when file times
/// are unchanged, without waiting for the old ten-second handle-cache expiry.
#[test]
fn native_reads_do_not_reuse_index_or_packed_refs_snapshots() -> Result<(), String> {
    let repo = Repo::new("native-fresh-snapshots")?;
    let base = repo.commit("base", &[("tracked", Some("base\n"))])?;
    let tip = repo.commit("tip", &[("tracked", Some("tip!\n"))])?;

    let reference = "refs/heads/native-snapshot";
    repo.git(&["update-ref", reference, &base])?;
    repo.git(&["pack-refs", "--all", "--prune"])?;
    let refs = &["for-each-ref", "--format=%(objectname)", reference];
    assert!(same(repo.path(), refs));
    let packed = repo.path().join(".git/packed-refs");
    let old_packed_time = std::fs::metadata(&packed)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?;

    repo.git(&["update-ref", reference, &tip])?;
    repo.git(&["pack-refs", "--all", "--prune"])?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&packed)
        .and_then(|f| f.set_modified(old_packed_time))
        .map_err(|e| e.to_string())?;
    assert!(same(repo.path(), refs));

    let path = repo.path().join("tracked");
    std::fs::write(&path, "one!\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "tracked"])?;
    assert_eq!(
        bounded_blob(repo.path(), ":0:tracked", 20),
        Some(Ok(b"one!\n".to_vec()))
    );
    let index = repo.path().join(".git/index");
    let old_index_time = std::fs::metadata(&index)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?;

    std::fs::write(&path, "two!\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "tracked"])?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&index)
        .and_then(|f| f.set_modified(old_index_time))
        .map_err(|e| e.to_string())?;
    assert_eq!(
        bounded_blob(repo.path(), ":0:tracked", 20),
        Some(Ok(b"two!\n".to_vec()))
    );
    assert!(same(repo.path(), STATUS));
    Ok(())
}

/// What git itself prints for `args` in `repo` (not through gitcraft, which would answer).
fn git(repo: &Path, args: &[&str]) -> Answer {
    let mut command = std::process::Command::new("git");
    for (name, value) in crate::git::command::GIT_ENV {
        command.env(name, value);
    }
    let out = match command.current_dir(repo).args(args).output() {
        Ok(out) => out,
        Err(e) => {
            return Answer {
                code: -1,
                stdout: Vec::new(),
                stderr: e.to_string().into_bytes(),
            };
        }
    };
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

/// Assert that each supported command is answered natively with byte-for-byte Git parity.
fn answered(repo: &Path, cases: &[&[&str]]) {
    for args in cases {
        assert!(same(repo, args), "not answered: git {args:?}");
    }
}

/// Assert that unsupported commands defer to the installed Git executable.
fn left_to_git(repo: &Path, cases: &[&[&str]]) {
    for args in cases {
        assert!(answer(repo, args).is_none(), "answered: git {args:?}");
    }
}

/// Compare native object, revision, and reference queries against Git.
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

/// Compare date-sorted branches and bisect output against Git.
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

/// Verify Git tree ordering for names that extend another path.
#[test]
fn tree_order_puts_folders_after_names_they_extend() {
    use std::cmp::Ordering::*;
    assert_eq!(tree_order(".github/dco.yml", false, "DCO", false), Less);
    assert_eq!(tree_order("a.b", false, "a/x", false), Less);
    assert_eq!(tree_order("a-b", false, "a", true), Less);
    assert_eq!(tree_order("a", true, "a.b", false), Greater);
    assert_eq!(tree_order("a", false, "a.b", false), Less);
}

#[path = "answer_tests/status.rs"]
mod status;

#[path = "answer_tests/config.rs"]
mod config;
