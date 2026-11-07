use super::*;
use std::path::PathBuf;
use std::process::Command;

struct Repo(PathBuf);
impl Repo {
    fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let r = Self(
            std::env::temp_dir().join(format!("gitcraft-activity-{}-{stamp}", std::process::id())),
        );
        std::fs::create_dir(&r.0).map_err(|e| e.to_string())?;
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Alias"],
            &["config", "user.email", "old@test.invalid"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            r.git(args)?;
        }
        Ok(r)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let o = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).trim_end().to_owned())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).into_owned())
        }
    }
    fn commit(&self, author: &str, committer: &str, msg: &str) -> Result<String, String> {
        self.git(&["add", "."])?;
        let o = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["commit", "-q", "--allow-empty", "-m", msg])
            .env("GIT_AUTHOR_DATE", author)
            .env("GIT_COMMITTER_DATE", committer)
            .output()
            .map_err(|e| e.to_string())?;
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stderr).into_owned());
        }
        self.git(&["rev-parse", "HEAD"])
    }
    fn read(&self, limit: usize) -> Result<History, String> {
        read(&self.0, limit, &|| true, &AtomicUsize::new(0))
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn aliases_refs_dates_renames_binary_and_empty_commits_share_one_snapshot() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("file\twith\nlines"), b"base\n").map_err(|e| e.to_string())?;
    let first = r.commit(
        "2026-01-01T23:30:00-1000",
        "2026-01-02T09:30:00+0000",
        "first",
    )?;
    r.git(&["branch", "duplicate"])?;
    r.git(&["tag", "also-first"])?;
    r.git(&["mv", "file\twith\nlines", "renamed"])?;
    std::fs::write(r.0.join("binary"), b"\0binary\xff").map_err(|e| e.to_string())?;
    let second = r.commit(
        "2025-12-01T00:30:00+1400",
        "2026-01-03T12:00:00+0000",
        "backdated",
    )?;
    r.git(&["config", "user.name", "Canonical"])?;
    r.git(&["config", "user.email", "new@test.invalid"])?;
    r.commit(
        "2026-01-04T00:00:00+0000",
        "2026-01-04T00:00:00+0000",
        "empty",
    )?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Canonical <new@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    let history = r.read(100)?;
    assert_eq!(history.commits.len(), 3);
    assert!(!history.more);
    let s = summarize(&history, Query::default(), &|| true)?;
    assert_eq!(s.contributors.len(), 1);
    let c = &s.contributors[0];
    assert_eq!((c.commits, c.files, c.days), (3, 3, 3));
    assert_eq!(c.name, "Canonical");
    assert_eq!(
        s.calendar.get(&Day::parse("2026-01-01").ok_or("date")?),
        Some(&1)
    );
    let q = Query {
        from: Day::parse("2025-12-01"),
        through: Day::parse("2025-12-01"),
        ..Default::default()
    };
    let s = summarize(&history, q, &|| true)?;
    assert_eq!(s.selected.len(), 1);
    assert_eq!(history.commits[s.selected[0]].hash, second);
    let s = summarize(
        &history,
        Query {
            clock: Clock::Committer,
            from: Day::parse("2026-01-02"),
            through: Day::parse("2026-01-02"),
        },
        &|| true,
    )?;
    assert_eq!(s.selected.len(), 1);
    assert_eq!(history.commits[s.selected[0]].hash, first);
    let c = history
        .commits
        .iter()
        .find(|c| c.hash == second)
        .ok_or("second")?;
    assert_eq!(c.paths.len(), 3);
    assert!(c.paths.contains(b"file\twith\nlines".as_slice()));
    assert!(c.paths.contains(b"binary".as_slice()));
    Ok(())
}

#[test]
fn empty_repositories_page_limits_and_cancellation_are_explicit() -> Result<(), String> {
    let r = Repo::new()?;
    assert!(r.read(100)?.commits.is_empty());
    for i in 0..4 {
        r.commit(
            "2026-01-01T00:00:00+0000",
            "2026-01-01T00:00:00+0000",
            &i.to_string(),
        )?;
    }
    let h = r.read(2)?;
    assert_eq!(h.commits.len(), 2);
    assert!(h.more);
    assert_eq!(r.read(4)?.commits.len(), 4);
    assert!(!r.read(4)?.more);
    assert!(
        read(&r.0, 100, &|| false, &AtomicUsize::new(0)).is_err_and(|e| e.contains("cancelled"))
    );
    assert!(summarize(&h, Query::default(), &|| false).is_err());
    Ok(())
}

#[test]
fn month_math_and_recorded_timezone_boundaries_are_validated() -> Result<(), String> {
    let d = Day::parse("2024-02-29").ok_or("leap")?;
    assert_eq!(d.weekday(), 3);
    assert_eq!(d.shift_month(1).to_string(), "2024-03-01");
    assert_eq!(Day::parse("2026-01-01").ok_or("day")?.weekday(), 3);
    assert!(Day::parse("2025-02-29").is_none());
    assert!(Day::parse("2026-13-01").is_none());
    assert!(Day::parse("2026-01-00").is_none());
    assert_eq!(
        Day::timestamp(b"2026-01-01T23:59:59-1200")?.to_string(),
        "2026-01-01"
    );
    assert_eq!(
        Day::timestamp(b"2026-01-02T00:00:00+1400")?.to_string(),
        "2026-01-02"
    );
    Ok(())
}

#[test]
fn nul_records_keep_non_utf8_paths_and_reject_incomplete_metadata() -> Result<(), String> {
    let mut data = format!(
        "\0{}\0A\0a@b\02026-01-01T00:00:00Z\02026-01-01T00:00:00Z\0subject\0\n:000000 100644 {} {} A\0",
        "a".repeat(40), "0".repeat(40), "b".repeat(40)
    )
    .into_bytes();
    data.extend_from_slice(b"odd\xff\t\nname\0");
    let parsed = parse(&data)?;
    assert!(parsed[0].paths.contains(b"odd\xff\t\nname".as_slice()));
    assert!(parse(b"\0abc\0").is_err());
    Ok(())
}

#[test]
fn merges_count_once_and_files_compare_with_the_first_parent() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("base"), "base").map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    let branch = r.git(&["branch", "--show-current"])?;
    r.git(&["checkout", "-qb", "side"])?;
    std::fs::write(r.0.join("side"), "side").map_err(|e| e.to_string())?;
    r.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "side")?;
    r.git(&["checkout", "-q", &branch])?;
    std::fs::write(r.0.join("main"), "main").map_err(|e| e.to_string())?;
    r.commit("2026-01-03T00:00:00Z", "2026-01-03T00:00:00Z", "main")?;
    r.git(&["merge", "-q", "--no-ff", "-m", "merge", "side"])?;
    let h = r.read(100)?;
    assert_eq!(h.commits.len(), 4);
    let merge = h
        .commits
        .iter()
        .find(|c| c.subject == "merge")
        .ok_or("merge")?;
    assert_eq!(merge.paths, BTreeSet::from([b"side".to_vec()]));
    Ok(())
}

/// A local fake SSH launcher exercises the real encoded-path shell protocol without a
/// network connection. Its environment is confined to this test's subprocess.
#[cfg(unix)]
#[test]
fn ssh_analysis_uses_the_encoded_repository_and_background_transport() -> Result<(), String> {
    const CHILD: &str = "GITCRAFT_ACTIVITY_SSH_TEST";
    if let Ok(path) = std::env::var(CHILD) {
        let encoded = crate::remote::encode(
            &crate::remote::Host::Ssh("activity-fixture.invalid".into()),
            &path,
        );
        let h = read(&encoded, 100, &|| true, &AtomicUsize::new(0))?;
        assert_eq!(h.commits.len(), 1);
        assert_eq!(h.commits[0].subject, "remote fixture");
        assert_eq!(h.commits[0].name, "Remote Canonical");
        let alias = "alias.activitylarge=!head -c 16777218 /dev/zero";
        let result = process::run(&encoded, &["-c", alias, "activitylarge"], None, &|| true);
        assert!(result.is_err_and(|e| e.contains("limit")));
        return Ok(());
    }
    let mut r = Repo::new()?;
    let quoted = r.0.with_file_name(format!(
        "{} remote's repo",
        r.0.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::rename(&r.0, &quoted).map_err(|e| e.to_string())?;
    r.0 = quoted;
    r.commit(
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        "remote fixture",
    )?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Remote Canonical <remote@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    let bin = r.0.join("bin");
    std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
    let ssh = bin.join("ssh");
    let marker = r.0.join("ssh-used");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nprintf '%s\\n' used >> {}\nexec /bin/sh\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let out=Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","git::activity::tests::ssh_analysis_uses_the_encoded_repository_and_background_transport","--nocapture"]).env(CHILD,&r.0).env("PATH",std::env::join_paths(paths).map_err(|e|e.to_string())?).output().map_err(|e|e.to_string())?;
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(marker.exists());
    Ok(())
}

#[test]
fn stash_and_application_refs_do_not_inflate_project_activity() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("work"), "base").map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "project")?;
    std::fs::write(r.0.join("work"), "stash").map_err(|e| e.to_string())?;
    r.git(&["stash", "push", "-qm", "private stash"])?;
    let tree = r.git(&["rev-parse", "HEAD^{tree}"])?;
    let orphan = r.git(&["commit-tree", &tree, "-m", "app snapshot"])?;
    r.git(&["update-ref", "refs/gitcraft/drafts/test", &orphan])?;
    r.git(&["update-ref", "refs/gitcraft/undo/test", &orphan])?;
    let h = r.read(100)?;
    assert_eq!(h.commits.len(), 1);
    assert_eq!(h.commits[0].subject, "project");
    // A real branch or tag exposes the same object as ordinary project history.
    r.git(&["tag", "visible-snapshot", &orphan])?;
    assert_eq!(r.read(100)?.commits.len(), 2);
    Ok(())
}

#[cfg(unix)]
#[test]
#[ignore = "Owned-process cancellation is gated separately by the parent containment review"]
fn cancelling_a_running_analysis_stops_process_descendants() -> Result<(), String> {
    use std::time::{Duration, Instant};
    let r = Repo::new()?;
    let marker = r.0.join("should-not-exist");
    let alias = format!(
        "alias.activitywait=!sleep 0.5; touch {}",
        crate::remote::sh_quote(&marker.to_string_lossy())
    );
    let start = Instant::now();
    let out = process::run(&r.0, &["-c", &alias, "activitywait"], None, &|| {
        start.elapsed() < Duration::from_millis(100)
    });
    assert!(out.is_err_and(|e| e.contains("cancelled")));
    assert!(start.elapsed() < Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(600));
    assert!(!marker.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
#[ignore = "Owned-process timeout is gated separately by the parent containment review"]
fn running_analysis_enforces_deadlines_and_output_limits() -> Result<(), String> {
    use std::time::{Duration, Instant};
    let r = Repo::new()?;
    let start = Instant::now();
    let result = super::super::command::bounded::local(
        &r.0,
        &["-c", "alias.activitywait=!sleep 30 & wait", "activitywait"],
        Vec::new(),
        64 * 1024 * 1024,
        Duration::from_millis(100),
    );
    assert!(result.is_err_and(|e| e.contains("timed out")));
    assert!(start.elapsed() < Duration::from_secs(2));
    let result = process::run(
        &r.0,
        &[
            "-c",
            "alias.activitylarge=!head -c 16777218 /dev/zero",
            "activitylarge",
        ],
        None,
        &|| true,
    );
    assert!(result.is_err_and(|e| e.contains("size limit") || e.contains("limit")));
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn real_git_keeps_non_utf8_file_paths_distinct() -> Result<(), String> {
    use std::os::unix::ffi::OsStringExt;
    let r = Repo::new()?;
    let raw = b"file\xff\t\nname".to_vec();
    let path = std::ffi::OsString::from_vec(raw.clone());
    std::fs::write(r.0.join(path), b"\0binary").map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "byte path")?;
    let h = r.read(100)?;
    assert!(h.commits[0].paths.contains(&raw));
    Ok(())
}

#[test]
fn analysis_does_not_change_dirty_worktree_index_or_refs() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("work"), "base").map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    std::fs::write(r.0.join("work"), "staged").map_err(|e| e.to_string())?;
    r.git(&["add", "work"])?;
    std::fs::write(r.0.join("work"), "unstaged").map_err(|e| e.to_string())?;
    let refs = r.git(&["show-ref"])?;
    let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
    let logs = r.git(&["reflog", "show", "--all"])?;
    let h = r.read(100)?;
    assert_eq!(h.commits.len(), 1);
    assert_eq!(r.git(&["show-ref"])?, refs);
    assert_eq!(r.git(&["reflog", "show", "--all"])?, logs);
    assert_eq!(
        std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read_to_string(r.0.join("work")).map_err(|e| e.to_string())?,
        "unstaged"
    );
    Ok(())
}

#[test]
fn blobless_history_analysis_does_not_download_file_contents() -> Result<(), String> {
    let source = Repo::new()?;
    std::fs::write(source.0.join("file"), "old contents").map_err(|e| e.to_string())?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "old")?;
    std::fs::write(source.0.join("file"), "new contents").map_err(|e| e.to_string())?;
    source.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "new")?;
    source.git(&["config", "uploadpack.allowFilter", "true"])?;
    let partial = Repo(source.0.join("partial"));
    let out = Command::new("git")
        .args(["clone", "--no-checkout", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&partial.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let missing = partial.git(&["rev-list", "--objects", "--all", "--missing=print"])?;
    assert!(missing.lines().any(|s| s.starts_with('?')));
    let history = partial.read(100)?;
    assert_eq!(history.commits.len(), 2);
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.paths.contains(b"file".as_slice()))
    );
    assert_eq!(
        partial.git(&["rev-list", "--objects", "--all", "--missing=print"])?,
        missing
    );
    Ok(())
}

#[test]
fn detached_head_commits_are_included_once() -> Result<(), String> {
    let r = Repo::new()?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&["checkout", "-q", "--detach"])?;
    let detached = r.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "detached")?;
    let h = r.read(100)?;
    assert_eq!(h.commits.len(), 2);
    assert_eq!(h.commits.iter().filter(|c| c.hash == detached).count(), 1);
    Ok(())
}

#[test]
fn shallow_history_reports_its_coverage_boundary() -> Result<(), String> {
    let source = Repo::new()?;
    std::fs::write(source.0.join("file"), "old").map_err(|e| e.to_string())?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    source.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "latest")?;
    let shallow = Repo(source.0.join("shallow"));
    let out = Command::new("git")
        .args(["clone", "--depth=1", "--no-local"])
        .arg(&source.0)
        .arg(&shallow.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let h = shallow.read(100)?;
    assert!(h.shallow);
    assert_eq!(h.commits.len(), 1);
    assert!(!h.more);
    Ok(())
}

fn chain(r: &Repo, n: usize) -> Result<(), String> {
    let mut parent = r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    let tree = r.git(&["rev-parse", "HEAD^{tree}"])?;
    for i in 1..n {
        parent = r.git(&["commit-tree", &tree, "-p", &parent, "-m", &i.to_string()])?;
    }
    let branch = r.git(&["symbolic-ref", "HEAD"])?;
    r.git(&["update-ref", &branch, &parent])?;
    Ok(())
}

#[test]
fn a_mailmap_edit_between_real_git_pages_cannot_split_an_author() -> Result<(), String> {
    let r = Repo::new()?;
    chain(&r, 270)?;
    let first = "First <first@test.invalid> Alias <old@test.invalid>\n";
    let second = "Second <second@test.invalid> Alias <old@test.invalid>\n";
    std::fs::write(r.0.join(".mailmap"), first).map_err(|e| e.to_string())?;
    let before = summarize(&r.read(1000)?, Query::default(), &|| true)?;
    assert_eq!(before.contributors.len(), 1);
    assert_eq!(before.contributors[0].name, "First");
    let mut edited = false;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE && !edited {
            assert!(std::fs::write(r.0.join(".mailmap"), second).is_ok());
            edited = true;
        }
    })?;
    assert!(edited);
    assert_eq!(history.commits.len(), 270);
    let result = summarize(&history, Query::default(), &|| true)?;
    assert_eq!(result.contributors.len(), 1);
    assert_eq!(result.contributors[0].name, "Second");
    assert_eq!(result.contributors[0].commits, 270);
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    // The same one-pass contract covers configured files and changing blob revisions.
    let extra = r.0.join("extra.map");
    std::fs::write(&extra, first).map_err(|e| e.to_string())?;
    r.git(&["config", "mailmap.file", "extra.map"])?;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE {
            assert!(std::fs::write(&extra, second).is_ok());
        }
    })?;
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    r.git(&["config", "mailmap.file", ""])?;
    std::fs::write(&extra, first).map_err(|e| e.to_string())?;
    let first_blob = r.git(&["hash-object", "-w", "--", "extra.map"])?;
    std::fs::write(&extra, second).map_err(|e| e.to_string())?;
    let second_blob = r.git(&["hash-object", "-w", "--", "extra.map"])?;
    r.git(&["config", "mailmap.blob", &first_blob])?;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE {
            assert!(r.git(&["config", "mailmap.blob", &second_blob]).is_ok());
        }
    })?;
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    Ok(())
}

#[test]
fn mailmap_blob_and_configured_file_precedence_match_git() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Blob <blob@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Dirty <dirty@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    let check = |repo: &Repo| -> Result<(), String> {
        let expected = repo.git(&["--no-lazy-fetch", "log", "-1", "--format=%aN <%aE>"])?;
        let h = repo.read(100)?;
        let c = &h.commits[0];
        assert_eq!(format!("{} <{}>", c.name, c.email), expected);
        Ok(())
    };
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Blob");
    std::fs::write(
        r.0.join("extra map"),
        "Configured <configured@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    r.git(&["config", "mailmap.file", "extra map"])?;
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Configured");
    // Relative augmenting files are relative to the worktree root, even from a subfolder.
    std::fs::create_dir(r.0.join("nested")).map_err(|e| e.to_string())?;
    let h = read(&r.0.join("nested"), 100, &|| true, &AtomicUsize::new(0))?;
    assert_eq!(h.commits[0].name, "Configured");
    r.git(&["config", "mailmap.file", ""])?;
    r.git(&["config", "mailmap.blob", ""])?;
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Dirty");
    Ok(())
}

#[test]
fn bare_mailmap_defaults_and_absent_optional_maps_match_git() -> Result<(), String> {
    let source = Repo::new()?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "unmapped")?;
    let bare = Repo(source.0.with_extension("bare"));
    let out = Command::new("git")
        .args(["clone", "--bare"])
        .arg(&source.0)
        .arg(&bare.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    assert_eq!(bare.read(100)?.commits[0].name, "Alias");
    std::fs::write(
        source.0.join(".mailmap"),
        "Canonical <canonical@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    source.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "mapped")?;
    bare.git(&["fetch", "-q", "origin", "+HEAD:refs/heads/mapped"])?;
    bare.git(&["symbolic-ref", "HEAD", "refs/heads/mapped"])?;
    let h = bare.read(100)?;
    assert!(h.commits.iter().all(|c| c.name == "Canonical"));
    let expected = bare.git(&["--no-lazy-fetch", "log", "-1", "--format=%aN <%aE>"])?;
    assert_eq!(
        format!("{} <{}>", h.commits[0].name, h.commits[0].email),
        expected
    );
    Ok(())
}

#[test]
fn unavailable_promised_mailmap_blob_is_an_error_without_downloading() -> Result<(), String> {
    let source = Repo::new()?;
    std::fs::write(
        source.0.join(".mailmap"),
        "Canonical <canonical@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "mapped")?;
    source.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    assert_eq!(source.read(100)?.commits[0].name, "Canonical");
    source.git(&["config", "uploadpack.allowFilter", "true"])?;
    let partial = Repo(source.0.join("partial"));
    let out = Command::new("git")
        .args(["clone", "--no-checkout", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&partial.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    partial.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    let missing = partial.git(&[
        "--no-lazy-fetch",
        "rev-list",
        "--objects",
        "--all",
        "--missing=print",
    ])?;
    assert!(missing.lines().any(|s| s.starts_with('?')));
    assert!(
        partial
            .read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    assert_eq!(
        partial.git(&[
            "--no-lazy-fetch",
            "rev-list",
            "--objects",
            "--all",
            "--missing=print"
        ])?,
        missing
    );
    let bare = Repo(source.0.join("partial-bare"));
    let out = Command::new("git")
        .args(["clone", "--bare", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&bare.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    let missing = bare.git(&[
        "--no-lazy-fetch",
        "rev-list",
        "--objects",
        "--all",
        "--missing=print",
    ])?;
    assert!(
        bare.read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    assert_eq!(
        bare.git(&[
            "--no-lazy-fetch",
            "rev-list",
            "--objects",
            "--all",
            "--missing=print"
        ])?,
        missing
    );
    Ok(())
}

#[test]
fn configured_missing_mailmap_file_and_successful_git_errors_are_reported() -> Result<(), String> {
    let r = Repo::new()?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&["config", "mailmap.file", "missing-map"])?;
    assert!(
        r.read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    #[cfg(unix)]
    {
        let result = process::run(
            &r.0,
            &[
                "-c",
                "alias.activitydiagnostic=!printf 'error: unavailable mailmap\\n' >&2; exit 0",
                "activitydiagnostic",
            ],
            None,
            &|| true,
        );
        assert!(result.is_err_and(|e| e.contains("unavailable mailmap")));
    }
    Ok(())
}

#[test]
fn configured_submodule_display_filters_never_hide_committed_gitlink_paths() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("file"), "base").map_err(|e| e.to_string())?;
    let oid = r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{oid},sub"),
    ])?;
    r.git(&["commit", "-qm", "gitlink"])?;
    r.git(&["config", "diff.relative", "true"])?;
    let nested = r.0.join("nested");
    std::fs::create_dir(&nested).map_err(|e| e.to_string())?;
    for value in ["none", "all", "dirty", "untracked"] {
        r.git(&["config", "diff.ignoreSubmodules", value])?;
        r.git(&["config", "submodule.sub.ignore", value])?;
        let h = read(&nested, 100, &|| true, &AtomicUsize::new(0))?;
        assert_eq!(h.commits.len(), 2);
        assert!(
            h.commits
                .iter()
                .any(|c| c.paths.contains(b"sub".as_slice()))
        );
        assert_eq!(
            summarize(&h, Query::default(), &|| true)?.contributors[0].files,
            2
        );
    }
    Ok(())
}
