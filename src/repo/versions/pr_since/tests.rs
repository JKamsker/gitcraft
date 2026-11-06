//! The version of a pull request seen, against a forge that is a repository here: kept across
//! a restart (it is in refs), and a force push found and described.

use super::*;

/// A folder removed when dropped.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Result<Scratch, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-since-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(Scratch(dir))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map(|s| s.trim().to_owned())
}

fn init(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(dir, args)?;
    }
    Ok(())
}

/// Fifty numbered lines, some replaced (by numbers, so no hunk names a function).
fn lines(changes: &[(usize, &str)]) -> String {
    (1..=50)
        .map(|n| {
            changes
                .iter()
                .find(|(m, _)| *m == n)
                .map_or(n.to_string(), |(_, w)| (*w).to_owned())
                + "\n"
        })
        .collect()
}

fn commit(dir: &Path, text: &str, message: &str) -> Result<String, String> {
    std::fs::write(dir.join("a.txt"), text).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", message])?;
    git(dir, &["rev-parse", "HEAD"])
}

/// The forge's repository, with pull request 7 as `refs/pull/7/head`, and a clone of it.
struct Hosted {
    _scratch: Scratch,
    forge: std::path::PathBuf,
    clone: std::path::PathBuf,
}

fn hosted(name: &str) -> Result<Hosted, String> {
    let scratch = scratch(name)?;
    let forge = scratch.0.join("forge");
    init(&forge)?;
    commit(&forge, &lines(&[]), "base")?;
    let clone = scratch.0.join("clone");
    git(
        &scratch.0,
        &["clone", "-q", &forge.to_string_lossy(), "clone"],
    )?;
    git(&clone, &["config", "commit.gpgSign", "false"])?;
    Ok(Hosted {
        _scratch: scratch,
        forge,
        clone,
    })
}

/// Pushes (forcing) the forge's `topic` as pull request 7; gives its head.
fn publish(forge: &Path) -> Result<String, String> {
    git(
        forge,
        &["update-ref", "refs/pull/7/head", "refs/heads/topic"],
    )?;
    git(forge, &["rev-parse", "refs/pull/7/head"])
}

fn pr(head: &str) -> PrHead {
    PrHead {
        number: 7,
        head: head.to_owned(),
        fetch_ref: "refs/pull/7/head".to_owned(),
        base: None,
        remote: Some("origin".to_owned()),
    }
}

const KEY: &str = "forge.example/acme/app";

#[test]
fn a_force_push_is_found_after_a_restart() -> Result<(), String> {
    let f = hosted("force-push")?;
    git(&f.forge, &["checkout", "-q", "-b", "topic"])?;
    commit(&f.forge, &lines(&[(5, "5555")]), "one")?;
    commit(&f.forge, &lines(&[(5, "5555"), (25, "2525")]), "two")?;
    let first = publish(&f.forge)?;

    // Looked at: remembered, fetched from the forge (the clone never had it).
    assert_eq!(look(&f.clone, KEY, &pr(&first), None)?, Found::First);
    assert_eq!(look(&f.clone, KEY, &pr(&first), None)?, Found::Same);
    assert_eq!(git::reviewed_heads(&f.clone, KEY)?, [(7, first.clone())]);

    // The author rewrites the second commit and adds a third, then force-pushes; the forge
    // no longer has the version seen.
    git(&f.forge, &["reset", "-q", "--hard", "HEAD~1"])?;
    commit(
        &f.forge,
        &lines(&[(5, "5555"), (25, "2002")]),
        "two, reworked",
    )?;
    commit(
        &f.forge,
        &lines(&[(5, "5555"), (25, "2002"), (45, "4545")]),
        "three",
    )?;
    let second = publish(&f.forge)?;
    git(&f.forge, &["reflog", "expire", "--expire=now", "--all"])?;
    git(&f.forge, &["gc", "-q", "--prune=now"])?;
    assert!(git(&f.forge, &["cat-file", "-e", &first]).is_err());

    // A new look, as after a restart: all it knows is in the refs.
    let found = look(&f.clone, KEY, &pr(&second), None)?;
    let Found::Moved(since) = found else {
        return Err(format!("not seen as moved: {found:?}"));
    };
    assert!(since.rewritten);
    assert_eq!(since.old.tip, first);
    assert_eq!(since.new.tip, second);
    assert_eq!(
        (since.counts.same, since.counts.changed, since.counts.added),
        (1, 1, 1)
    );
    assert_eq!(
        super::super::summary(&since.counts),
        "1 commit changed, 1 added, 1 identical"
    );
    // The version seen is still here to compare with.
    assert!(
        git(
            &f.clone,
            &["cat-file", "-e", &format!("{first}^{{commit}}")]
        )
        .is_ok()
    );

    // Seen now: the next look finds nothing new, and the version before stays reachable.
    let marked = mark_seen(&f.clone, KEY, &pr(&second), None)?;
    assert_eq!(marked.tip, second);
    assert_eq!(look(&f.clone, KEY, &pr(&second), None)?, Found::Same);
    git(&f.clone, &["reflog", "expire", "--expire=now", "--all"])?;
    git(&f.clone, &["gc", "-q", "--prune=now"])?;
    assert!(
        git(
            &f.clone,
            &["cat-file", "-e", &format!("{first}^{{commit}}")]
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn new_commits_on_top_are_not_a_rewrite() -> Result<(), String> {
    let f = hosted("fast-forward")?;
    git(&f.forge, &["checkout", "-q", "-b", "topic"])?;
    commit(&f.forge, &lines(&[(5, "5555")]), "one")?;
    let first = publish(&f.forge)?;
    assert_eq!(look(&f.clone, KEY, &pr(&first), None)?, Found::First);
    commit(&f.forge, &lines(&[(5, "5555"), (30, "3030")]), "two")?;
    let second = publish(&f.forge)?;
    let Found::Moved(since) = look(&f.clone, KEY, &pr(&second), None)? else {
        return Err("not seen as moved".to_owned());
    };
    assert!(!since.rewritten);
    assert_eq!((since.counts.same, since.counts.added), (1, 1));
    Ok(())
}

#[test]
fn what_the_forge_names_is_checked_before_git_gets_it() -> Result<(), String> {
    let f = hosted("hostile")?;
    let mut bad = pr("--upload-pack=touch x");
    assert!(look(&f.clone, KEY, &bad, None).is_err());
    // A refspec that would write a branch is left out of the fetch, not given to git.
    bad.head = "f".repeat(40);
    bad.fetch_ref = "+refs/pull/7/head:refs/heads/main".to_owned();
    assert!(look(&f.clone, KEY, &bad, None).is_err());
    assert!(!is_plain_ref(&bad.fetch_ref));
    assert!(is_plain_ref("refs/merge-requests/3/head"));
    assert!(!is_plain_ref("--all"));
    assert_eq!(
        pull_ref(Some(Forge::GitLab), 3),
        "refs/merge-requests/3/head"
    );
    assert_eq!(pull_ref(Some(Forge::Gitea), 3), "refs/pull/3/head");
    Ok(())
}
