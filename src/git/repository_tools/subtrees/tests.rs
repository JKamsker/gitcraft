use super::*;
fn source() -> Source {
    Source {
        id: "source-1".into(),
        prefix: "vendor/a folder".into(),
        remote: "upstream".into(),
        branch: "main".into(),
        history: History::Squash,
        fetched: Some("a".repeat(40)),
        imported: None,
    }
}
fn candidate() -> Candidate {
    Candidate {
        id: "attempt-1".into(),
        source: "source-1".into(),
        branch: "refs/heads/dev".into(),
        old: "b".repeat(40),
        upstream: Some("a".repeat(40)),
        kind: Kind::Update,
        stage: Stage::Ready,
        worktree: PathBuf::from("/repo/.git/gitcraft-subtree-attempt-1-random/candidate"),
        identity: Some("12:34".into()),
        result: Some("c".repeat(40)),
        notice: "retained".into(),
    }
}
#[test]
fn subtree_pure_registry_rejects_overlap_traversal_and_foreign_candidates() {
    for p in [
        "",
        "/vendor",
        "../vendor",
        "vendor/../elsewhere",
        "vendor/.git/x",
        "vendor/.GIT",
        "a//b",
        "a\\b",
        "a/",
        "-option",
        "a\nb",
    ] {
        assert!(!prefix(p), "{p:?}");
    }
    assert!(prefix("vendor/a folder"));
    let mut r = Registry {
        sources: vec![source()],
        candidates: vec![candidate()],
    };
    assert!(validate(&r, Path::new("/repo/.git")).is_ok());
    r.candidates[0].worktree = PathBuf::from("/foreign/candidate");
    assert!(validate(&r, Path::new("/repo/.git")).is_err());
    r.candidates.clear();
    let mut other = source();
    other.id = "other".into();
    other.prefix = "vendor/a folder/nested".into();
    r.sources.push(other);
    assert!(validate(&r, Path::new("/repo/.git")).is_err());
}
#[test]
fn subtree_pure_helper_recording_distinguishes_history_and_split() {
    let mut s = source();
    let old = "b".repeat(40);
    let upstream = "a".repeat(40);
    assert_eq!(
        subtree_words(&s, Kind::Update, &old, Some(&upstream), false).unwrap(),
        vec![
            "subtree",
            "add",
            "--prefix=vendor/a folder",
            "--squash",
            upstream.as_str()
        ]
    );
    assert_eq!(
        subtree_words(&s, Kind::Update, &old, Some(&upstream), true).unwrap()[1],
        "merge"
    );
    s.history = History::Full;
    let full = subtree_words(&s, Kind::Update, &old, Some(&upstream), true).unwrap();
    assert!(!full.iter().any(|w| w == "--squash"));
    assert_eq!(full.last(), Some(&upstream));
    assert_eq!(
        subtree_words(&s, Kind::Split, &old, None, true).unwrap(),
        vec!["subtree", "split", "--prefix=vendor/a folder", old.as_str()]
    );
    assert!(subtree_words(&s, Kind::Update, &old, None, true).is_err());
}
#[test]
fn subtree_pure_recorded_fetch_and_push_pin_exact_objects() {
    let s = source();
    let pin = "refs/gitcraft/subtrees/fetch/attempt-1";
    let words = fetch_words(&s, pin);
    assert!(words.iter().any(|w| w == "--no-write-fetch-head"));
    assert!(words.iter().any(|w| w == "--no-recurse-submodules"));
    assert_eq!(
        &words[words.len() - 3..],
        &[
            "--",
            "upstream",
            "refs/heads/main:refs/gitcraft/subtrees/fetch/attempt-1"
        ]
    );
    assert!(!words.iter().any(|w| w == "--all" || w == "--force"));
    let mut c = candidate();
    c.kind = Kind::Split;
    c.stage = Stage::SplitReady;
    let ops = push_words(&s, &c).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].last(),
        Some(&format!("{}:refs/heads/main", "c".repeat(40)))
    );
    assert!(
        !ops[0]
            .iter()
            .any(|w| w == "HEAD" || w.starts_with("--force") || w == "--set-upstream")
    );
}
#[test]
fn subtree_pure_acceptance_requires_exact_clean_original_receipt() {
    let mut c = candidate();
    assert!(accept_allowed(&c, &c.branch, &c.old, true).is_ok());
    assert!(accept_allowed(&c, &c.branch, &c.old, false).is_err());
    assert!(accept_allowed(&c, "refs/heads/other", &c.old, true).is_err());
    assert!(accept_allowed(&c, &c.branch, &"e".repeat(40), true).is_err());
    for stage in [
        Stage::Preparing,
        Stage::Running,
        Stage::Conflict,
        Stage::Accepting,
        Stage::Accepted,
        Stage::Paused,
        Stage::Closed,
    ] {
        c.stage = stage;
        assert!(accept_allowed(&c, &c.branch, &c.old, true).is_err());
    }
    c.stage = Stage::Ready;
    c.kind = Kind::Split;
    assert!(accept_allowed(&c, &c.branch, &c.old, true).is_err());
}
#[test]
fn subtree_pure_ambiguous_sends_require_renewed_human_review() {
    let mut c = candidate();
    c.kind = Kind::Split;
    for stage in [Stage::Sending, Stage::Paused] {
        c.stage = stage;
        assert!(!publishable(&c, false));
        assert!(publishable(&c, true));
    }
    c.stage = Stage::SplitReady;
    assert!(publishable(&c, false));
    for stage in [Stage::Sent, Stage::Closed, Stage::Running, Stage::Preparing] {
        c.stage = stage;
        assert!(!publishable(&c, false));
        assert!(!publishable(&c, true));
    }
    c.stage = Stage::Sending;
    let r = Registry {
        sources: vec![source()],
        candidates: vec![c],
    };
    let saved = serde_json::to_vec(&r).unwrap();
    let restored: Registry = serde_json::from_slice(&saved).unwrap();
    assert_eq!(restored.candidates[0].stage, Stage::Sending);
    assert!(!publishable(&restored.candidates[0], false));
    assert_eq!(restored.candidates[0].old, "b".repeat(40));
    assert_eq!(restored.candidates[0].identity.as_deref(), Some("12:34"));
}
#[test]
fn subtree_pure_state_bound_is_enforced_before_candidate_adoption() {
    let mut r = Registry {
        sources: vec![source()],
        candidates: vec![candidate(); 65],
    };
    assert!(validate(&r, Path::new("/repo/.git")).is_err());
    r.candidates.truncate(1);
    r.candidates[0].old = "HEAD".into();
    assert!(validate(&r, Path::new("/repo/.git")).is_err());
    r.candidates[0] = candidate();
    r.sources[0].fetched = Some("main".into());
    assert!(validate(&r, Path::new("/repo/.git")).is_err());
}
impl View {
    pub(crate) fn subtree_test_view() -> Self {
        Self {
            state: Registry {
                sources: vec![source()],
                candidates: vec![candidate()],
            },
            version: "d".repeat(40),
            branch: "refs/heads/dev".into(),
            head: "b".repeat(40),
            remotes: vec!["upstream".into()],
            local: Some(Local {
                source: "source-1".into(),
                baseline: Some("a".repeat(40)),
                committed: "+local vendor edit".into(),
                staged: "+staged vendor edit".into(),
                working: "+later edit".into(),
                untracked: vec!["vendor/a folder/local.txt".into()],
            }),
            candidate_diff: Some(("attempt-1".into(), "+proposed update".into())),
            common: PathBuf::from("/repo/.git"),
            route: String::new(),
            helper: "Supplied helper observation only".into(),
        }
    }
}
#[test]
fn subtree_pure_association_records_reviewed_baseline_without_changing_tree() {
    let mut s = source();
    let mut c = candidate();
    c.kind = Kind::Associate;
    let base = c.upstream.as_deref().unwrap();
    let squash = baseline_words(&s, base).unwrap();
    assert_eq!(squash[0], "commit-tree");
    assert_eq!(squash[1], format!("{base}^{{tree}}"));
    assert!(!squash.iter().any(|w| w == "-p"));
    let condensed = "d".repeat(40);
    let join = association_words(&c, &s, &condensed, base);
    assert_eq!(join[1], format!("{}^{{tree}}", c.old));
    assert_eq!(
        &join[2..6],
        &["-p", c.old.as_str(), "-p", condensed.as_str()]
    );
    assert!(join[7].contains(&format!("git-subtree-mainline: {}", c.old)));
    assert!(join[7].contains(&format!("git-subtree-split: {base}")));
    s.history = History::Full;
    assert!(baseline_words(&s, base).is_none());
    let full = association_words(&c, &s, base, base);
    assert_eq!(full[5], base);
    assert!(accept_allowed(&c, &c.branch, &c.old, true).is_ok());
    for name in ["a:b", "refs/../victim", "a.lock", "a\nb", "a b"] {
        assert!(!branch_name(name));
    }
}
