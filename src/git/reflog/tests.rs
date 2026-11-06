//! Losing commits in real repositories (a hard reset, a deleted branch, a rebase) and finding
//! them again in the reflog, or with the scan once the reflog has expired.

use std::path::{Path, PathBuf};

use super::lost::CANCELLED;
use super::*;
use crate::git::{head, run};

/// A fresh repository in the temporary folder, removed when dropped.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-reflog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.logAllRefUpdates", "true"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.0), args)
            .map(|out| out.trim().to_owned())
            .map_err(|e| format!("git {args:?}: {e}"))
    }

    /// An empty commit made at `time` (Unix seconds, as author and committer), its hash.
    fn commit_at(&self, message: &str, time: i64) -> Result<String, String> {
        let date = format!("@{time} +0000");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output();
        if !out.is_ok_and(|o| o.status.success()) {
            return Err(format!("could not commit {message}"));
        }
        Ok(head(&self.0))
    }

    /// A commit adding a line to a file of its own, so that it can be cherry-picked.
    fn commit(&self, message: &str) -> Result<String, String> {
        let file = self
            .0
            .join(format!("{}.txt", head(&self.0).get(..8).unwrap_or("root")));
        std::fs::write(&file, format!("{message}\n")).map_err(|e| e.to_string())?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "-m", message])?;
        Ok(head(&self.0))
    }

    /// Forgets every reflog entry of commits no ref reaches, as `git gc` does once they are
    /// old enough.
    fn expire_reflogs(&self) -> Result<String, String> {
        self.git(&[
            "reflog",
            "expire",
            "--expire=now",
            "--expire-unreachable=now",
            "--all",
        ])
    }

    fn lost(&self) -> Result<Vec<String>, String> {
        let found = lost_commits(&self.0, &Cancel::default())?;
        Ok(found.into_iter().map(|c| c.hash).collect())
    }

    fn read(&self, reference: &str) -> Result<ReflogPage, String> {
        reflog(&self.0, reference, 0, REFLOG_PAGE)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_hard_reset_is_in_heads_reflog_with_what_it_threw_away() -> Result<(), String> {
    let repo = Repo::new("reset")?;
    let base = repo.commit("base")?;
    let lost = repo.commit("work to lose")?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    let page = repo.read("HEAD")?;
    assert!(!page.more);
    let kinds: Vec<ReflogKind> = page.entries.iter().map(|e| e.message.kind).collect();
    assert_eq!(
        kinds,
        [ReflogKind::Reset, ReflogKind::Commit, ReflogKind::Initial]
    );
    let reset = &page.entries[0];
    assert_eq!(reset.selector, "HEAD@{0}");
    assert_eq!(reset.message.detail, "moving to HEAD~1");
    assert_eq!(
        (reset.old.as_deref(), reset.new.as_str()),
        (Some(lost.as_str()), base.as_str())
    );
    assert_eq!(page.entries[1].summary, "work to lose");
    assert_eq!(page.entries[1].parents, std::slice::from_ref(&base));
    // The first entry made the ref: it came from nowhere.
    assert_eq!(page.entries[2].old, None);
    assert!(
        page.entries
            .iter()
            .all(|e| e.time > 0 && e.date.len() == 16)
    );
    // The branch's own reflog says the same.
    let main = repo.read("refs/heads/main")?;
    assert_eq!(main.entries[0].old.as_deref(), Some(lost.as_str()));
    assert_eq!(main.entries[0].selector, "main@{0}");
    // The scan doesn't count the reflogs: it finds it now, and once they have expired.
    assert_eq!(repo.lost()?, std::slice::from_ref(&lost));
    repo.expire_reflogs()?;
    assert!(repo.read("HEAD")?.entries.iter().all(|e| e.new != lost));
    assert_eq!(repo.lost()?, std::slice::from_ref(&lost));
    // Getting it back as a branch, as the view's Create branch here does.
    repo.git(&["branch", "recovered", &lost])?;
    assert_eq!(repo.git(&["rev-parse", "recovered"])?, lost);
    assert!(repo.lost()?.is_empty());
    Ok(())
}

#[test]
fn a_deleted_branch_lives_on_in_heads_reflog_and_then_in_the_scan() -> Result<(), String> {
    let repo = Repo::new("branch-d")?;
    repo.commit("base")?;
    repo.git(&["switch", "-q", "-c", "topic"])?;
    let first = repo.commit("topic one")?;
    let tip = repo.commit("topic two")?;
    repo.git(&["switch", "-q", "main"])?;
    repo.git(&["branch", "-q", "-D", "topic"])?;
    // The branch's reflog went with it.
    assert_eq!(repo.read("refs/heads/topic")?, ReflogPage::default());
    let page = repo.read("HEAD")?;
    let checkout = &page.entries[0];
    assert_eq!(checkout.message.kind, ReflogKind::Checkout);
    assert_eq!(checkout.message.detail, "moving from topic to main");
    assert_eq!(checkout.old.as_deref(), Some(tip.as_str()));
    repo.expire_reflogs()?;
    // Both, the newest first.
    let lost = repo.lost()?;
    assert_eq!(lost.len(), 2);
    assert!(lost.contains(&first) && lost.contains(&tip));
    // Cherry-picking one back, as the view's Cherry-pick does.
    repo.git(&["cherry-pick", &first])?;
    assert_eq!(repo.git(&["log", "-1", "--format=%s"])?, "topic one");
    Ok(())
}

#[test]
fn a_rebase_leaves_the_old_tip_in_the_branchs_reflog() -> Result<(), String> {
    let repo = Repo::new("rebase")?;
    let base = repo.commit("base")?;
    repo.git(&["switch", "-q", "-c", "topic"])?;
    let dropped = repo.commit("dropped by the rebase")?;
    let kept = repo.commit("kept")?;
    // Rebased onto base without its first commit: that one is gone from the branch.
    repo.git(&["rebase", "-q", "--onto", &base, &dropped, "topic"])?;
    let topic = repo.read("refs/heads/topic")?;
    let finish = &topic.entries[0];
    assert_eq!(finish.message.kind, ReflogKind::Rebase);
    assert_eq!(finish.old.as_deref(), Some(kept.as_str()));
    let heads = repo.read("HEAD")?;
    assert!(
        heads
            .entries
            .iter()
            .any(|e| e.message.action.starts_with("rebase"))
    );
    assert!(heads.entries.iter().any(|e| e.new == dropped));
    // Checked out detached, as the view's Check out does, the old commit is back.
    repo.git(&["checkout", "-q", &kept, "--"])?;
    assert_eq!(head(repo.path()), kept);
    repo.git(&["switch", "-q", "topic"])?;
    repo.expire_reflogs()?;
    let lost = repo.lost()?;
    assert!(lost.contains(&dropped) && lost.contains(&kept), "{lost:?}");
    Ok(())
}

#[test]
fn long_reflogs_come_page_by_page_each_knowing_where_it_came_from() -> Result<(), String> {
    let repo = Repo::new("pages")?;
    let commits = (0..5)
        .map(|i| repo.commit(&format!("c{i}")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = Vec::new();
    let mut skip = 0;
    loop {
        let page = reflog(repo.path(), "HEAD", skip, 2)?;
        assert!(page.entries.len() <= 2);
        skip += page.entries.len();
        seen.extend(page.entries);
        if !page.more {
            break;
        }
    }
    let news: Vec<&str> = seen.iter().map(|e| e.new.as_str()).collect();
    let mut want: Vec<&str> = commits.iter().map(String::as_str).collect();
    want.reverse();
    assert_eq!(news, want);
    for pair in seen.windows(2) {
        assert_eq!(pair[0].old.as_deref(), Some(pair[1].new.as_str()));
    }
    let selectors: Vec<&str> = seen.iter().map(|e| e.selector.as_str()).collect();
    assert_eq!(
        selectors,
        ["HEAD@{0}", "HEAD@{1}", "HEAD@{2}", "HEAD@{3}", "HEAD@{4}"]
    );
    assert_eq!(seen.last().and_then(|e| e.old.clone()), None);
    Ok(())
}

#[test]
fn nothing_to_read_is_an_empty_reflog() -> Result<(), String> {
    let repo = Repo::new("empty")?;
    // No commits yet.
    assert_eq!(repo.read("HEAD")?, ReflogPage::default());
    repo.commit("base")?;
    assert_eq!(
        repo.read("refs/heads/no-such-branch")?,
        ReflogPage::default()
    );
    // A branch made without a reflog.
    repo.git(&["-c", "core.logAllRefUpdates=false", "branch", "quiet"])?;
    assert_eq!(repo.read("refs/heads/quiet")?, ReflogPage::default());
    // A ref name that looks like an option is a ref.
    assert_eq!(repo.read("--output=x")?, ReflogPage::default());
    assert!(!repo.path().join("x").exists());
    assert!(repo.lost()?.is_empty());
    Ok(())
}

#[test]
fn unicode_in_messages_and_branch_names_comes_through() -> Result<(), String> {
    let repo = Repo::new("unicode")?;
    repo.commit("Ünïcödé 修正 🚀")?;
    repo.git(&["switch", "-q", "-c", "fünf/日本"])?;
    repo.commit("auf dem Zweig: ja")?;
    let page = repo.read("HEAD")?;
    assert_eq!(page.entries[0].message.detail, "auf dem Zweig: ja");
    assert_eq!(
        page.entries[1].message.detail,
        "moving from main to fünf/日本"
    );
    assert_eq!(page.entries[2].summary, "Ünïcödé 修正 🚀");
    let branch = repo.read("refs/heads/fünf/日本")?;
    assert_eq!(branch.entries.len(), 2);
    assert!(branch.entries[0].selector.ends_with("@{0}"));
    Ok(())
}

/// An entry whose commit is gone is left out, and the one after it doesn't claim to come from
/// the one before.
#[test]
fn a_gap_in_the_reflog_leaves_where_it_came_from_unknown() -> Result<(), String> {
    let repo = Repo::new("gap")?;
    let base = repo.commit("base")?;
    let gone = repo.commit("gone")?;
    repo.git(&["reset", "-q", "--hard", &base])?;
    let object = repo
        .path()
        .join(".git/objects")
        .join(&gone[..2])
        .join(&gone[2..]);
    // git leaves its objects read-only, which Windows won't delete.
    let mut writable = std::fs::metadata(&object)
        .map_err(|e| e.to_string())?
        .permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    writable.set_readonly(false);
    std::fs::set_permissions(&object, writable).map_err(|e| e.to_string())?;
    std::fs::remove_file(&object).map_err(|e| e.to_string())?;
    let page = repo.read("HEAD")?;
    let selectors: Vec<&str> = page.entries.iter().map(|e| e.selector.as_str()).collect();
    assert_eq!(selectors, ["HEAD@{0}", "HEAD@{2}"]);
    // The reset did come from `gone`, which was left out: not from base.
    assert_eq!(page.entries[0].old, None);
    Ok(())
}

#[test]
fn the_scan_lists_lost_commits_newest_first() -> Result<(), String> {
    let repo = Repo::new("scan-order")?;
    repo.commit_at("base", 1_700_000_000)?;
    let older = repo.commit_at("older", 1_700_000_100)?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    let newer = repo.commit_at("newer", 1_700_000_200)?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    repo.expire_reflogs()?;
    let found = lost_commits(repo.path(), &Cancel::default())?;
    let hashes: Vec<&str> = found.iter().map(|c| c.hash.as_str()).collect();
    assert_eq!(hashes, [newer.as_str(), older.as_str()]);
    assert_eq!(found[0].summary, "newer");
    assert_eq!(found[0].author, "t");
    assert_eq!(found[0].time, 1_700_000_200);
    assert_eq!(found[0].parents.len(), 1);
    Ok(())
}

/// A scan cancelled while git runs stops it and says so.
#[test]
fn a_scan_cancelled_while_it_runs_says_so() -> Result<(), String> {
    let repo = Repo::new("scan-cancel")?;
    repo.commit("base")?;
    let cancel = Cancel::default();
    let scanning = {
        let cancel = cancel.clone();
        let path = repo.path().to_owned();
        std::thread::spawn(move || lost_commits(&path, &cancel))
    };
    cancel.cancel();
    let result = scanning.join().ok();
    // Either it was stopped, or it had finished first: never a half-read list.
    assert!(
        matches!(&result, Some(Err(e)) if e == CANCELLED)
            || matches!(&result, Some(Ok(found)) if found.is_empty()),
        "{result:?}"
    );
    assert!(cancel.is_cancelled());
    Ok(())
}
