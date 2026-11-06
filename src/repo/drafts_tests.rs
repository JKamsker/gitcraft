//! What a tab typed comes back in a tab opened again on the same repository, through the paths
//! the window takes, on real repositories; and goes once it is used.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::RepoTab;
use super::pr_review::{LineTarget, Posted, PrDraft};
use super::rebase::{self, Step};
use crate::drafts::{self, RepoDrafts};
use crate::git;
use crate::pull_request::DiffSide;

/// A repository with three commits, in a fresh folder with a space and accents in its name.
fn repo(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft drafts ünï {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "root"],
        &["commit", "-q", "--allow-empty", "-m", "two"],
        &["commit", "-q", "--allow-empty", "-m", "three"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// Saves what `tab` has, as the window does, and waits for it to be on disk.
fn save(tab: &RepoTab) {
    for (path, drafts) in tab.all_drafts() {
        drafts::save(&path, drafts);
    }
    drafts::flush(Duration::from_secs(10));
}

/// Polls `tab` until `done`, for up to half a minute. Whether it got there.
fn settle(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// The drafts the next start would find for `dir`: its file's.
fn on_disk(dir: &Path) -> RepoDrafts {
    drafts::from_disk(dir)
}

#[test]
fn the_commit_message_comes_back_and_goes_once_committed() {
    let dir = repo("commit");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_summary = "Fix ünïcode in paths with spaces 🦀".to_owned();
    tab.commit_body = "First line\n\n  indented\ttab".to_owned();
    save(&tab);

    let mut again = RepoTab::new_deferred(dir.clone(), &ctx);
    assert_eq!(again.commit_summary, tab.commit_summary);
    assert_eq!(again.commit_body, tab.commit_body);
    assert!(!again.amend);

    // Committed through the commit box's path: the message is used up.
    let _ = std::fs::write(dir.join("new file.txt"), "x\n");
    again.commit(&ctx);
    let committed = settle(&mut again, &ctx, |t| {
        t.busy_op.is_none() && t.commit_summary.is_empty()
    });
    let subject = git::head_message(&dir).unwrap_or_default();
    save(&again);
    let left = on_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(committed);
    assert!(subject.starts_with("Fix ünïcode"), "{subject}");
    assert_eq!(left, RepoDrafts::default());
}

#[test]
fn a_commit_that_fails_keeps_its_message() {
    let dir = repo("commit-fails");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_summary = "Nothing to commit".to_owned();
    // Nothing changed: git refuses the commit.
    tab.commit(&ctx);
    let done = settle(&mut tab, &ctx, |t| t.busy_op.is_none());
    save(&tab);
    let left = on_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(done);
    assert_eq!(left.commit.summary, "Nothing to commit");
}

#[test]
fn an_interactive_rebase_list_comes_back_and_goes_once_started() {
    let dir = repo("rebase");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let commits = git::rebase_commits(&dir, "HEAD~2").unwrap_or_default();
    let mut editor = rebase::RebaseEditor::new("HEAD~2".to_owned(), "HEAD~2".to_owned(), commits);
    // Reordered, the older one reworded: what was set up is what comes back.
    editor.entries.swap(0, 1);
    editor.entries[1].step = Step::Reword;
    editor.entries[1].message = "two, reworded ✓".to_owned();
    tab.rebase = Some(editor.clone());
    save(&tab);

    let mut again = RepoTab::new_deferred(dir.clone(), &ctx);
    let restored = again.rebase.clone();
    let same = restored.as_ref().is_some_and(|r| {
        r.onto == editor.onto
            && r.head == editor.head
            && r.entries.len() == 2
            && r.entries
                .iter()
                .zip(&editor.entries)
                .all(|(a, b)| a.commit == b.commit && a.step == b.step && a.message == b.message)
    });

    // Started through the editor's Start: the list is used up.
    again.refs.head = git::head(&dir);
    if let Some(r) = again.rebase.as_mut() {
        r.entries.swap(0, 1);
        r.entries[0].step = Step::Pick;
    }
    again.run_rebase(&ctx);
    let started = settle(&mut again, &ctx, |t| {
        t.rebase.is_none() && t.busy_op.is_none()
    });
    save(&again);
    let left = on_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(same, "{:?}", restored.map(|r| r.entries.len()));
    assert!(started);
    assert!(left.rebase.is_none());
}

#[test]
fn a_rebase_list_cancelled_is_not_kept() {
    let dir = repo("rebase-cancel");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
    tab.rebase = Some(rebase::RebaseEditor::new(
        "HEAD~1".to_owned(),
        "HEAD~1".to_owned(),
        commits,
    ));
    save(&tab);
    assert!(on_disk(&dir).rebase.is_some());
    tab.apply(super::Action::CloseRebase, &ctx);
    save(&tab);
    let left = on_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(left, RepoDrafts::default());
}

#[test]
fn unsent_pull_request_text_comes_back_and_goes_once_sent() {
    let dir = repo("pr");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.pr_drafts.insert(
        5,
        PrDraft {
            comment: "Thanks!\nOne question".to_owned(),
            review: "Looks good".to_owned(),
            line_text: "A reply, “quoted”".to_owned(),
            ..PrDraft::default()
        },
    );
    // Nothing typed: nothing kept.
    tab.pr_drafts.insert(6, PrDraft::default());
    save(&tab);

    let mut again = RepoTab::new_deferred(dir.clone(), &ctx);
    let restored = again.pr_drafts.get(&5);
    let same = restored.is_some_and(|d| {
        d.comment == "Thanks!\nOne question"
            && d.review == "Looks good"
            && d.line_text == "A reply, “quoted”"
    });
    let kept_empty = again.pr_drafts.contains_key(&6);

    // The comment is sent: only it goes.
    again.pr_posted(5, Posted::Comment, Ok(()), &ctx);
    save(&again);
    let after_comment = on_disk(&dir);
    again.pr_posted(
        5,
        Posted::Review(crate::pr_post::Verdict::Comment),
        Ok(()),
        &ctx,
    );
    again.pr_posted(5, Posted::Reply { pending: false }, Ok(()), &ctx);
    save(&again);
    let after_all = on_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(same);
    assert!(!kept_empty);
    let text = after_comment
        .pull_requests
        .get(&5)
        .cloned()
        .unwrap_or_default();
    assert_eq!(text.comment, "");
    assert_eq!(text.review, "Looks good");
    assert_eq!(after_all, RepoDrafts::default());
}

#[test]
fn a_line_comment_keeps_its_line_only_while_the_pull_request_has_not_moved() {
    let dir = repo("pr-line");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let target = LineTarget {
        path: "src/main.rs".to_owned(),
        line: 40,
        side: DiffSide::Left,
    };
    tab.pr_drafts.insert(
        3,
        PrDraft {
            target: Some(target.clone()),
            line_text: "Off by one".to_owned(),
            ..PrDraft::default()
        },
    );
    // The page isn't open, so which commits the line was picked in isn't known: the text is
    // kept, not the line.
    let without_page = tab.drafts();
    let text = without_page
        .pull_requests
        .get(&3)
        .cloned()
        .unwrap_or_default();
    assert_eq!(text.line_text, "Off by one");
    assert!(text.line.is_none());

    // Put back with the line and the commit it was picked at.
    let mut drafts = RepoDrafts::default();
    drafts.pull_requests.insert(
        3,
        crate::drafts::PrText {
            line_text: "Off by one".to_owned(),
            line: Some(crate::drafts::LineDraft {
                path: "src/main.rs".to_owned(),
                line: 40,
                new_side: false,
                head: "abc".to_owned(),
            }),
            ..Default::default()
        },
    );
    let mut again = RepoTab::new_deferred(dir.clone(), &ctx);
    again.put_back(drafts);
    let _ = std::fs::remove_dir_all(&dir);
    let back = again.pr_drafts.get(&3);
    assert!(back.is_some_and(|d| d.target == Some(target.clone())));
    assert!(back.is_some_and(|d| d.line_head.as_deref() == Some("abc")));
}

#[test]
fn two_repositories_keep_their_own_drafts() {
    let (a, b) = (repo("apart-a"), repo("apart-b"));
    let ctx = egui::Context::default();
    let mut tab_a = RepoTab::new_deferred(a.clone(), &ctx);
    let mut tab_b = RepoTab::new_deferred(b.clone(), &ctx);
    tab_a.commit_summary = "Only in a".to_owned();
    tab_b.commit_summary = "Only in b".to_owned();
    tab_b.amend = true;
    save(&tab_a);
    save(&tab_b);
    let (again_a, again_b) = (
        RepoTab::new_deferred(a.clone(), &ctx),
        RepoTab::new_deferred(b.clone(), &ctx),
    );
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
    assert_eq!(again_a.commit_summary, "Only in a");
    assert!(!again_a.amend);
    assert_eq!(again_b.commit_summary, "Only in b");
    assert!(again_b.amend);
}

#[test]
fn an_empty_message_and_ticking_amend_alone_keep_nothing() {
    let dir = repo("empty");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_summary = "   ".to_owned();
    tab.amend = true;
    let drafts = tab.drafts();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(drafts.is_empty());
}

#[test]
fn sign_ins_typed_in_the_tab_are_never_kept() {
    let dir = repo("secrets");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.set_credentials(git::Credentials {
        username: "me".to_owned(),
        password: "hunter2-token".to_owned(),
    });
    tab.commit_summary = "Visible".to_owned();
    let json = serde_json::to_string(&tab.drafts()).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(json.contains("Visible"));
    assert!(!json.contains("hunter2"));
}

#[test]
fn a_huge_message_is_kept_in_part() {
    let dir = repo("huge");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_summary = "Big".to_owned();
    tab.commit_body = "ü".repeat(1024 * 1024);
    save(&tab);
    let file = drafts::from_disk(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(file.commit.summary, "Big");
    assert!(file.commit.body.len() < tab.commit_body.len());
    assert!(file.commit.body.len() >= 128 * 1024);
    assert!(file.commit.body.chars().all(|c| c == 'ü'));
}
