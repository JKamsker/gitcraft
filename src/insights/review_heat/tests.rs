use super::*;
use crate::git::fixture::Repo;
use crate::pr_threads::ThreadComment;

fn hunk(old_start: u32, old_len: u32, new_start: u32, new_len: u32) -> Hunk {
    Hunk {
        old_start,
        old_len,
        new_start,
        new_len,
    }
}

fn file(path: &str, hunks: Vec<Hunk>) -> FileDiff {
    FileDiff {
        path: path.to_owned(),
        old_path: None,
        status: 'M',
        added: Some(1),
        removed: Some(1),
        hunks,
    }
}

fn thread_at(path: &str, line: Option<u64>, left: bool, resolved: bool) -> ThreadAt {
    ThreadAt {
        path: path.to_owned(),
        line,
        left,
        resolved,
        outdated: false,
        author: "r".to_owned(),
        text: "why?".to_owned(),
        url: String::new(),
    }
}

#[test]
fn hunks_are_read_by_file() {
    let out = "diff --git a/a.rs b/a.rs\nindex 1..2 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -3 +3,2 @@ fn x()\n-old\n+new\n+more\n@@ -10,2 +11,0 @@\n-gone\n-gone\ndiff --git a/b.txt b/b.txt\ndeleted file mode 100644\n--- a/b.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-x\n-y\n";
    let found = parse_hunks(out);
    assert_eq!(found["a.rs"], [hunk(3, 1, 3, 2), hunk(10, 2, 11, 0)]);
    assert_eq!(found["b.txt"], [hunk(1, 2, 0, 0)]);
}

#[test]
fn only_the_hunks_touched_since_the_version_seen_are_changed() {
    let files = vec![
        file("a.rs", vec![hunk(1, 2, 1, 3), hunk(20, 1, 21, 4)]),
        file("b.rs", vec![hunk(5, 1, 5, 1)]),
    ];
    let seen = Seen::Fresh {
        tip: "t".into(),
        since: [("a.rs".to_owned(), vec![hunk(22, 0, 22, 2)])].into(),
    };
    let heat = derive(files, &seen, Some(&[]));
    assert_eq!(heat[0].seen, SeenMark::Changed);
    assert_eq!(heat[0].since, Some(2));
    assert_eq!(heat[0].hunks[0].seen, SeenMark::Same);
    assert_eq!(heat[0].hunks[0].since, Some(0));
    assert_eq!(heat[0].hunks[1].seen, SeenMark::Changed);
    assert_eq!(heat[0].hunks[1].since, Some(2));
    assert_eq!(heat[1].seen, SeenMark::Same);
    assert_eq!(heat[1].hunks[0].seen.evidence(), Evidence::Observed);
}

#[test]
fn threads_land_on_the_hunk_of_their_line_and_side() {
    let files = vec![file("a.rs", vec![hunk(1, 2, 1, 3), hunk(20, 1, 21, 4)])];
    let mut outdated = thread_at("a.rs", Some(2), false, false);
    outdated.outdated = true;
    let threads = vec![
        thread_at("a.rs", Some(22), false, false),
        // Line 20 of the old side: the second hunk.
        thread_at("a.rs", Some(20), true, false),
        thread_at("a.rs", Some(2), false, true),
        outdated,
        thread_at("a.rs", Some(50), false, false),
        thread_at("other.rs", Some(1), false, false),
    ];
    let heat = derive(files, &Seen::Never, Some(&threads));
    let f = &heat[0];
    assert_eq!(f.threads, [0, 1, 2, 3, 4]);
    assert_eq!(f.hunks[0].threads, [2]);
    assert_eq!(f.hunks[1].threads, [0, 1]);
    let h = Heat {
        threads: Some(threads),
        ..Heat::default()
    };
    // The resolved one isn't open.
    assert_eq!(h.open(&f.hunks[0].threads), 0);
    assert_eq!(h.open(&f.threads), 4);
}

#[test]
fn what_isnt_known_stays_unknown() {
    let files = vec![file("a.rs", vec![hunk(1, 1, 1, 1)])];
    let heat = derive(files.clone(), &Seen::Unknown("no forge".into()), None);
    assert_eq!(heat[0].seen, SeenMark::Unknown);
    assert_eq!(heat[0].seen.evidence(), Evidence::Unknown);
    assert_eq!(heat[0].since, None);
    assert_eq!(heat[0].hunks[0].seen, SeenMark::Unknown);
    assert!(heat[0].threads.is_empty());
    // Never seen: not seen here, nothing to count changes from.
    let heat = derive(files, &Seen::Never, Some(&[]));
    assert_eq!(heat[0].seen, SeenMark::NotSeen);
    assert_eq!(heat[0].hunks[0].since, None);
}

#[test]
fn after_a_rewrite_only_the_files_left_as_seen_are_inferred_the_same() {
    let files = vec![
        file("a.rs", vec![hunk(1, 1, 1, 1)]),
        file("b.rs", vec![hunk(1, 1, 1, 1)]),
    ];
    let seen = Seen::Stale {
        tip: "t".into(),
        differing: ["a.rs".to_owned()].into(),
    };
    let heat = derive(files, &seen, Some(&[]));
    assert_eq!(heat[0].seen, SeenMark::Stale);
    assert_eq!(heat[0].hunks[0].seen, SeenMark::Stale);
    assert_eq!(heat[1].seen, SeenMark::SameAfterRewrite);
    assert_eq!(heat[1].seen.evidence(), Evidence::Inferred);
    assert!(stale_reason(&seen, "h").is_some());
    assert_eq!(stale_reason(&Seen::Never, "h"), None);
}

fn thread(path: &str, line: u64) -> Thread {
    Thread {
        id: "t1".into(),
        path: path.into(),
        line: Some(line),
        start_line: None,
        side: DiffSide::Right,
        outdated: false,
        resolved: false,
        can_resolve: false,
        can_unresolve: false,
        can_reply: false,
        comments: vec![ThreadComment {
            id: "c1".into(),
            author: "rev".into(),
            at: 0,
            body: "Is this right?\nmore".into(),
            url: "https://example.org/c1".into(),
            pending: false,
        }],
        starts_left: false,
        more_comments: false,
    }
}

pub(crate) const KEY: &str = "example.org/o/r";

/// A repository with `main` and a pull request branch of two commits, as seen in gitcraft at
/// its head; the details the forge would give for it at `head`.
pub(crate) fn pr_repo(name: &str) -> Result<(Repo, String, String), String> {
    let repo = Repo::new(name)?;
    let lines: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    let base = repo.commit("base", &[("a.txt", Some(&lines)), ("b.txt", Some("b\n"))])?;
    repo.git(&["checkout", "-q", "-b", "pr"])?;
    let edited = lines.replace("line 3\n", "line three\n");
    repo.commit("edit a", &[("a.txt", Some(&edited))])?;
    let head = repo.commit("edit b", &[("b.txt", Some("b changed\n"))])?;
    git::mark_reviewed(repo.path(), KEY, 5, &git::Series::new(&base, &head))?;
    Ok((repo, base, head))
}

pub(crate) fn details(base: &str, head: &str) -> PrDetails {
    let mut d = PrDetails {
        number: 5,
        base_branch: "main".into(),
        base_oid: base.into(),
        head_branch: "pr".into(),
        head_oid: head.into(),
        ..PrDetails::default()
    };
    d.extras.head_oid = head.into();
    d.extras.threads = vec![thread("a.txt", 3)];
    d
}

fn marks(heat: &Heat) -> Vec<(String, SeenMark)> {
    heat.files
        .iter()
        .map(|f| (f.file.path.clone(), f.seen))
        .collect()
}

#[test]
fn new_commits_on_the_head_are_changed_since_the_version_seen() -> Result<(), String> {
    let (repo, base, head) = pr_repo("review-heat-moved")?;
    let heat = analyze(repo.path(), &details(&base, &head), Some(KEY))?;
    assert_eq!(
        marks(&heat),
        [
            ("a.txt".to_owned(), SeenMark::Same),
            ("b.txt".to_owned(), SeenMark::Same)
        ]
    );
    assert_eq!(heat.files[0].hunks.len(), 1);
    assert_eq!(heat.files[0].hunks[0].threads, [0]);
    assert_eq!(heat.open(&heat.files[0].threads), 1);
    // The author pushes a commit changing line 25 of a.txt.
    let lines: String = (1..=30)
        .map(|n| match n {
            3 => "line three\n".to_owned(),
            25 => "line twenty-five\n".to_owned(),
            _ => format!("line {n}\n"),
        })
        .collect();
    let moved = repo.commit("edit a again", &[("a.txt", Some(&lines))])?;
    let mut provenance = heat.provenance.clone();
    provenance.check_tips(|name| {
        if name == head_name(5) {
            Some(moved.clone())
        } else {
            Some(head.clone())
        }
    });
    assert!(provenance.stale.is_some());
    let heat = analyze(repo.path(), &details(&base, &moved), Some(KEY))?;
    assert_eq!(heat.seen_tip(), Some(head.as_str()));
    assert_eq!(heat.stale_review, None);
    let a = &heat.files[0];
    assert_eq!(a.seen, SeenMark::Changed);
    // The hunk seen stays as seen; the new one is changed since.
    let by_line: Vec<(u32, SeenMark)> =
        a.hunks.iter().map(|h| (h.hunk.new_start, h.seen)).collect();
    assert_eq!(by_line, [(3, SeenMark::Same), (25, SeenMark::Changed)]);
    assert_eq!(heat.files[1].seen, SeenMark::Same);
    // The threads were read at the old head: where they are is a guess.
    let mut elsewhere = details(&base, &moved);
    elsewhere.extras.head_oid = head.clone();
    let heat = analyze(repo.path(), &elsewhere, Some(KEY))?;
    assert!(heat.threads_elsewhere);
    assert!(heat.provenance.partial.is_some());
    Ok(())
}

#[test]
fn a_force_push_makes_the_version_seen_stale() -> Result<(), String> {
    let (repo, base, head) = pr_repo("review-heat-forced")?;
    // The branch rewritten from main: a.txt as it was seen, b.txt otherwise.
    repo.git(&["checkout", "-q", "-B", "pr", &base])?;
    let lines: String = (1..=30)
        .map(|n| {
            if n == 3 {
                "line three\n".to_owned()
            } else {
                format!("line {n}\n")
            }
        })
        .collect();
    let forced = repo.commit(
        "edit a and b",
        &[("a.txt", Some(&lines)), ("b.txt", Some("b rewritten\n"))],
    )?;
    let heat = analyze(repo.path(), &details(&base, &forced), Some(KEY))?;
    assert!(matches!(&heat.seen, Some(Seen::Stale { tip, .. }) if *tip == head));
    let why = heat.stale_review.as_deref().unwrap_or_default();
    assert!(why.contains("force-pushed"), "{why}");
    assert_eq!(
        marks(&heat),
        [
            ("a.txt".to_owned(), SeenMark::SameAfterRewrite),
            ("b.txt".to_owned(), SeenMark::Stale)
        ]
    );
    assert!(heat.files.iter().all(|f| f.since.is_none()));
    Ok(())
}

#[test]
fn without_a_forge_or_commits_nothing_is_claimed() -> Result<(), String> {
    let (repo, base, head) = pr_repo("review-heat-noforge")?;
    let heat = analyze(repo.path(), &details(&base, &head), None)?;
    assert!(matches!(heat.seen, Some(Seen::Unknown(_))));
    assert!(heat.files.iter().all(|f| f.seen == SeenMark::Unknown));
    // Threads that couldn't be read are unknown, not none.
    let mut no_threads = details(&base, &head);
    no_threads.inline_error = Some("offline".into());
    let heat = analyze(repo.path(), &no_threads, Some(KEY))?;
    assert!(heat.threads.is_none());
    assert!(heat.provenance.partial.is_some());
    // Commits that aren't here aren't fetched.
    let missing = "0123456789012345678901234567890123456789";
    let err = analyze(repo.path(), &details(&base, missing), Some(KEY));
    assert!(err.is_err_and(|e| e.contains("open the pull request's page")));
    // Not even ids: refused.
    assert!(analyze(repo.path(), &details("main", "--all"), Some(KEY)).is_err());
    Ok(())
}
