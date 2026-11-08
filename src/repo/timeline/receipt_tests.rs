//! Receipts of real rewrites, made through the tab as the UI makes them: a rebase onto another
//! branch, a squash and a reorder with the interactive rebase's todo list, an amend that changes
//! the author of a signed commit; and the reading of range-diff and of raw commits.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::receipt::{PairKind, Receipt, parse_range_diff, parse_raw};
use super::store;
use crate::git;
use crate::repo::actions::{BranchRef, rebase_action};
use crate::repo::{Action, Refresh, RepoTab, Undo, ops};

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn range_diff_headers_are_read_with_full_hashes() {
    let out = format!(
        "1:  {A} = 1:  {B} f1\n2:  {B} ! 2:  {A} f2 “ü”\n    @@ Metadata\n3:  {A} < -:  ---------------------------------------- gone\n-:  ---------------------------------------- > 3:  {B} new\n"
    );
    let headers = parse_range_diff(&out);
    let kinds: Vec<PairKind> = headers.iter().map(|h| h.kind).collect();
    assert_eq!(
        kinds,
        [
            PairKind::Same,
            PairKind::Changed,
            PairKind::Dropped,
            PairKind::Added
        ]
    );
    assert_eq!(headers[2].new, None);
    assert_eq!(headers[3].old, None);
    assert_eq!(headers[0].old.as_deref(), Some(A));
    assert!(parse_range_diff("").is_empty());
}

#[test]
fn raw_commits_are_read_with_their_signatures() {
    let out = format!(
        "commit {A}\ntree {B}\nparent {B}\nauthor Jöhn Doe <j@d> 1700000000 +0100\ncommitter C <c@c> 1700000000 +0100\ngpgsig -----BEGIN SSH SIGNATURE-----\n abc\n -----END SSH SIGNATURE-----\n\n    Fix the “parser”\n\n    Body line\n\ncommit {B}\ntree {A}\nauthor X <x@x> 1 +0000\ncommitter X <x@x> 1 +0000\n\n    other\n"
    );
    let list = parse_raw(&out);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].author, "Jöhn Doe <j@d>");
    assert_eq!(list[0].tree, B);
    assert!(list[0].signed);
    assert_eq!(list[0].summary, "Fix the “parser”");
    assert_eq!(list[0].message, "Fix the “parser”\n\nBody line");
    // Ten pairs and more: the positions are padded.
    let padded = format!(" 9:  {A} =  9:  {B} nine\n10:  {B} ! 10:  {A} ten\n    @@ body\n");
    let headers = parse_range_diff(&padded);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].kind, PairKind::Same);
    assert!(!list[1].signed);
    assert_eq!(list[1].message, "other");
}

fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft rc ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Tëst"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        git_ok(&dir, args);
    }
    if let Some(file) = store::file(&dir) {
        let _ = std::fs::remove_file(file);
    }
    dir
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "{args:?}: {out:?}");
    out.unwrap_or_default().trim().to_owned()
}

fn commit_file(dir: &Path, file: &str, message: &str) -> String {
    let _ = std::fs::write(dir.join(file), format!("{message}\n"));
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", message]);
    git::head(dir)
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    // This waits for Git and the subsequent asynchronous snapshot, not a performance
    // budget. Windows Git process startup under shard load can exceed ten seconds.
    while start.elapsed() < Duration::from_secs(30) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn settled(tab: &RepoTab) -> bool {
    !tab.loading
        && tab.busy_op.is_none()
        && tab.history_gen > 0
        && tab.timeline.loaded
        && tab.timeline.awaiting == super::Awaiting::Nothing
}

/// Runs `action` in a tab on `dir` and gives the receipt of the entry it made, once read.
fn receipt_of(dir: &Path, action: Action) -> (Option<Receipt>, String) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.to_owned(), &ctx);
    assert!(wait(&mut tab, &ctx, settled));
    let review = match &action {
        Action::AskRebase { onto, .. } => Some((
            tab.refs
                .current
                .clone()
                .unwrap_or_else(|| unreachable!("no branch")),
            git::head(dir),
            git_ok(dir, &["rev-parse", onto]),
        )),
        _ => None,
    };
    tab.apply(action, &ctx);
    if let Some((branch, head, onto)) = review {
        let accepted = tab
            .accept_rebase_for_test(&ctx, &branch, &head, &onto)
            .unwrap_or_else(|error| unreachable!("rebase confirmation: {error}"));
        tab.apply(accepted, &ctx);
    }
    assert!(
        wait(&mut tab, &ctx, settled),
        "status={:?}, loading={}, busy={}, history={}, timeline_loaded={}, awaiting={:?}",
        tab.status,
        tab.loading,
        tab.busy_op.is_some(),
        tab.history_gen,
        tab.timeline.loaded,
        tab.timeline.awaiting
    );
    let id = tab.timeline.current.unwrap_or_default();
    // The background work that would have made one is done (it names commits too).
    wait(&mut tab, &ctx, |t| {
        t.timeline
            .get(id)
            .is_some_and(|e| e.label.contains('“') || e.receipt.is_some() || !e.files.is_empty())
    });
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    // Written with it.
    wait(&mut tab, &ctx, |t| {
        store::load(&t.path).is_ok_and(|s| {
            s.entries.iter().any(|e| {
                e.id == id && e.receipt == t.timeline.get(id).and_then(|x| x.receipt.clone())
            })
        })
    });
    (tab.timeline.get(id).and_then(|e| e.receipt.clone()), status)
}

/// The interactive rebase as the editor starts it: a todo list handed over as the sequence
/// editor.
fn interactive(dir: &Path, onto: &str, todo: &str) -> Action {
    let file = std::env::temp_dir().join(format!(
        "gitcraft-rc-todo-{}-{}",
        std::process::id(),
        dir.file_name()
            .map(|n| n.to_string_lossy().len())
            .unwrap_or(0)
    ));
    let _ = std::fs::write(&file, todo);
    let file = file.to_string_lossy().replace('\\', "/");
    Action::UndoableOps {
        label: "Interactive rebase".to_owned(),
        ops: ops(&[&[
            "-c",
            &format!("sequence.editor=cp '{file}'"),
            "rebase",
            "-i",
            onto,
        ]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// A rebase onto another branch: the two commits are the same changes on a new base, the
/// files differ only by what the base brought, and main's commit is not taken for one of the
/// branch's; the receipt is kept on disk with the entry, and the status line says it.
#[test]
fn a_rebase_follows_each_commit_to_its_copy_on_the_new_base() {
    let dir = repo("rebase");
    git_ok(&dir, &["checkout", "-q", "-b", "feat"]);
    let f1 = commit_file(&dir, "f1.txt", "f1 “ü”");
    let f2 = commit_file(&dir, "f2.txt", "f2");
    git_ok(&dir, &["checkout", "-q", "main"]);
    commit_file(&dir, "m.txt", "m1");
    git_ok(&dir, &["checkout", "-q", "feat"]);
    let (receipt, status) = receipt_of(&dir, rebase_action(BranchRef::local("main"), "feat"));
    let kept = store::load(&dir)
        .ok()
        .and_then(|s| s.entries.last().and_then(|e| e.receipt.clone()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = receipt.unwrap_or_else(|| unreachable!("no receipt"));
    assert_eq!(r.branch, "feat");
    assert_eq!(r.old_tip, f2);
    assert!(!r.same_files);
    let olds: Vec<&str> = r
        .pairs
        .iter()
        .filter_map(|p| p.old.as_ref())
        .map(|c| c.hash.as_str())
        .collect();
    assert_eq!(olds, [f1.as_str(), f2.as_str()]);
    assert!(
        r.pairs.iter().all(|p| p.kind == PairKind::Same),
        "{:?}",
        r.pairs
    );
    assert_eq!(
        r.sentence(),
        "Final files differ, authors kept, messages kept"
    );
    assert_eq!(kept, Some(r.clone()));
    assert!(
        status.contains("Final files differ, authors kept"),
        "{status}"
    );
}

/// Two commits squashed into one: the files are the same, one message changed, one commit
/// dropped into the other.
#[test]
fn a_squash_says_the_files_are_identical_and_a_commit_went() {
    let dir = repo("squash");
    let f1 = commit_file(&dir, "f1.txt", "one");
    let f2 = commit_file(&dir, "f2.txt", "two");
    let base = git_ok(&dir, &["rev-parse", "HEAD~2"]);
    let todo = format!("pick {f1} one\nsquash {f2} two\n");
    let (receipt, _) = receipt_of(&dir, interactive(&dir, &base, &todo));
    let _ = std::fs::remove_dir_all(&dir);
    let r = receipt.unwrap_or_else(|| unreachable!("no receipt"));
    assert!(r.same_files);
    let c = r.counts();
    assert_eq!(c.same + c.changed + c.dropped, 2, "{:?}", r.pairs);
    assert_eq!(c.same + c.changed + c.added, 1, "{:?}", r.pairs);
    assert!(
        r.sentence().starts_with("Final files identical"),
        "{}",
        r.sentence()
    );
    assert!(r.sentence().ends_with("2 commits → 1"), "{}", r.sentence());
    // Squashed into the first: paired with it, its message grown by the second's.
    if c.changed == 1 {
        assert_eq!(c.messages_changed, 1);
        assert!(r.sentence().contains("1 message changed"));
    }
}

/// Two commits swapped: the same changes in another order, the same final files.
#[test]
fn a_reorder_keeps_every_change_and_the_files() {
    let dir = repo("reorder");
    let f1 = commit_file(&dir, "f1.txt", "one");
    let f2 = commit_file(&dir, "f2.txt", "two");
    let base = git_ok(&dir, &["rev-parse", "HEAD~2"]);
    let todo = format!("pick {f2} two\npick {f1} one\n");
    let (receipt, _) = receipt_of(&dir, interactive(&dir, &base, &todo));
    let _ = std::fs::remove_dir_all(&dir);
    let r = receipt.unwrap_or_else(|| unreachable!("no receipt"));
    assert!(r.same_files);
    assert_eq!(r.pairs.len(), 2);
    assert!(
        r.pairs.iter().all(|p| p.kind == PairKind::Same),
        "{:?}",
        r.pairs
    );
    assert_eq!(
        r.sentence(),
        "Final files identical, authors kept, messages kept"
    );
}

/// A signed commit amended under another author: the signature is gone, and so is the author.
#[test]
fn an_amend_says_the_author_and_signature_changed() {
    let dir = repo("amend");
    commit_file(&dir, "a.txt", "signed one");
    // A signature as git stores it (whether it verifies doesn't matter here).
    let raw = git_ok(&dir, &["cat-file", "commit", "HEAD"]);
    let (headers, message) = raw.split_once("\n\n").unwrap_or((&raw, ""));
    let signed = format!(
        "{headers}\ngpgsig -----BEGIN SSH SIGNATURE-----\n U1NIU0lH\n -----END SSH SIGNATURE-----\n\n{message}\n"
    );
    let file = dir.join(".git").join("signed-commit");
    let _ = std::fs::write(&file, signed);
    let made = git_ok(
        &dir,
        &["hash-object", "-t", "commit", "-w", &file.to_string_lossy()],
    );
    git_ok(&dir, &["reset", "-q", "--hard", &made]);
    let amend = Action::UndoableOps {
        label: "Amend".to_owned(),
        ops: ops(&[&["commit", "--amend", "--no-edit", "--author", "Ötto <o@o>"]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--soft" },
    };
    let (receipt, _) = receipt_of(&dir, amend);
    let _ = std::fs::remove_dir_all(&dir);
    let r = receipt.unwrap_or_else(|| unreachable!("no receipt"));
    assert_eq!(r.old_tip, made);
    assert!(r.same_files);
    assert_eq!(r.pairs.len(), 1);
    assert!(r.pairs[0].author_changed());
    assert!(r.pairs[0].old.as_ref().is_some_and(|c| c.signed));
    assert_eq!(
        r.sentence(),
        "Final files identical, 1 author changed, messages kept, 1 signature dropped"
    );
}

/// A plain commit rewrites nothing: no receipt.
#[test]
fn a_commit_has_no_receipt() {
    let dir = repo("plain");
    let _ = std::fs::write(dir.join("a.txt"), "a");
    let commit = Action::UndoableOps {
        label: "Commit".to_owned(),
        ops: ops(&[&["add", "-A"], &["commit", "-q", "-m", "a"]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--soft" },
    };
    let (receipt, _) = receipt_of(&dir, commit);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(receipt, None);
}

/// Resetting a branch back to an older commit of its own drops commits but rewrites none: no
/// receipt.
#[test]
fn a_reset_back_has_no_receipt() {
    let dir = repo("reset-back");
    commit_file(&dir, "a.txt", "a");
    let reset = Action::UndoableOps {
        label: "Reset main to HEAD~1".to_owned(),
        ops: ops(&[&["reset", "-q", "--hard", "HEAD~1"]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    };
    let (receipt, _) = receipt_of(&dir, reset);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(receipt, None);
}

/// A rebase of a branch pushed earlier and grown since, onto a main that moved: the commit
/// the remote has (on the old series) is still the branch's own, not upstream; main's is.
#[test]
fn a_ref_on_an_old_commit_does_not_cut_the_series() {
    let dir = repo("pushed");
    git_ok(&dir, &["checkout", "-q", "-b", "feat"]);
    for n in 1..=11 {
        commit_file(&dir, &format!("f{n}.txt"), &format!("f{n}"));
        if n == 2 {
            git_ok(&dir, &["update-ref", "refs/remotes/origin/feat", "HEAD"]);
            git_ok(&dir, &["tag", "v-old"]);
        }
    }
    git_ok(&dir, &["checkout", "-q", "main"]);
    commit_file(&dir, "m.txt", "m1");
    git_ok(&dir, &["checkout", "-q", "feat"]);
    let (receipt, _) = receipt_of(&dir, rebase_action(BranchRef::local("main"), "feat"));
    let _ = std::fs::remove_dir_all(&dir);
    let r = receipt.unwrap_or_else(|| unreachable!("no receipt"));
    let c = r.counts();
    assert_eq!(
        (c.same, c.changed, c.added, c.dropped),
        (11, 0, 0, 0),
        "{:?}",
        r.pairs
    );
    assert_eq!(
        r.sentence(),
        "Final files differ, authors kept, messages kept"
    );
}
