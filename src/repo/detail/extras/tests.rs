//! The sections through the tab, as the commit pane drives them, on a real repository: what
//! is read once a commit is selected, notes written and deleted, namespaces picked, answers
//! for a commit no longer shown dropped, and the sections drawn.

use std::path::{Path, PathBuf};

use super::{Extras, ExtrasCmd};
use crate::git::{self, LinkKind, SignatureVerdict};
use crate::repo::{Action, RepoTab};

fn repo(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-tab-extras-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "tag.gpgSign", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

fn commit(dir: &Path, message: &str) -> String {
    assert!(git::run(Some(dir), &["commit", "-q", "--allow-empty", "-m", message]).is_ok());
    git::run(Some(dir), &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

/// Takes in background answers until `done` says the tab has what it waits for.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&Extras) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        tab.extras_tick(ctx);
        if done(&tab.extras) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn all_read(e: &Extras) -> bool {
    e.signature.is_some()
        && e.contained.is_some()
        && e.backlinks.is_some()
        && e.notes.note.is_some()
        && !e.notes.saving
}

fn note_text(e: &Extras) -> Option<Option<&str>> {
    match &e.notes.note {
        Some(Ok(note)) => Some(note.as_deref()),
        _ => None,
    }
}

#[test]
fn selecting_a_commit_reads_its_sections_and_notes_round_trip() {
    let dir = repo("round-trip");
    let target = commit(&dir, "the target");
    assert!(git::run(Some(&dir), &["tag", "v1.0"]).is_ok());
    let fix = commit(&dir, &format!("Fix\n\nFixes: {}", &target[..12]));
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);

    tab.load_extras(&target, &ctx);
    wait(&mut tab, &ctx, all_read);
    let e = &tab.extras;
    assert!(matches!(&e.signature, Some(Ok(s)) if s.verdict == SignatureVerdict::Unsigned));
    assert!(matches!(&e.contained, Some(Ok(c)) if c.first_tag() == Some("v1.0")));
    let links: Vec<(&str, &LinkKind)> = match &e.backlinks {
        Some(Ok((links, false))) => links.iter().map(|l| (&l.hash[..], &l.kind)).collect(),
        _ => Vec::new(),
    };
    assert_eq!(links, [(&fix[..], &LinkKind::Trailer("Fixes".to_owned()))]);
    assert_eq!(note_text(e), Some(None));
    assert_eq!(e.notes.namespace.as_deref(), Some("commits"));

    // Saved, then shown.
    let mut acts = Vec::new();
    tab.extras_cmd(
        ExtrasCmd::SaveNote("Reviewed ✓".to_owned()),
        &mut acts,
        &ctx,
    );
    assert!(tab.extras.notes.saving);
    wait(&mut tab, &ctx, |e| {
        note_text(e) == Some(Some("Reviewed ✓\n"))
    });
    assert_eq!(note_text(&tab.extras), Some(Some("Reviewed ✓\n")));

    // Another namespace: empty at first, its own note, and the first one still there.
    tab.extras_cmd(
        ExtrasCmd::PickNamespace(" review/ci ".to_owned()),
        &mut acts,
        &ctx,
    );
    wait(&mut tab, &ctx, all_read);
    assert_eq!(tab.extras.notes.namespace.as_deref(), Some("review/ci"));
    assert_eq!(note_text(&tab.extras), Some(None));
    tab.extras_cmd(ExtrasCmd::SaveNote("CI green".to_owned()), &mut acts, &ctx);
    wait(&mut tab, &ctx, |e| note_text(e) == Some(Some("CI green\n")));
    assert_eq!(tab.extras.notes.namespaces, ["commits", "review/ci"]);
    tab.extras_cmd(
        ExtrasCmd::PickNamespace("commits".to_owned()),
        &mut acts,
        &ctx,
    );
    wait(&mut tab, &ctx, |e| {
        note_text(e) == Some(Some("Reviewed ✓\n"))
    });
    assert_eq!(note_text(&tab.extras), Some(Some("Reviewed ✓\n")));

    // A name git can't take is refused, and the namespace shown stays.
    tab.extras_cmd(
        ExtrasCmd::PickNamespace("bad name".to_owned()),
        &mut acts,
        &ctx,
    );
    assert!(
        tab.extras
            .notes
            .error
            .as_deref()
            .is_some_and(|e| e.contains("spaces"))
    );
    assert_eq!(tab.extras.notes.namespace.as_deref(), Some("commits"));

    // Deleted.
    tab.extras_cmd(ExtrasCmd::DeleteNote, &mut acts, &ctx);
    wait(&mut tab, &ctx, |e| {
        note_text(e) == Some(None) && !e.notes.saving
    });
    assert_eq!(note_text(&tab.extras), Some(None));

    // Fetching and pushing are operations, with the namespace on both sides.
    tab.extras_cmd(ExtrasCmd::PushNotes("origin".to_owned()), &mut acts, &ctx);
    tab.extras_cmd(ExtrasCmd::FetchNotes("origin".to_owned()), &mut acts, &ctx);
    tab.extras_cmd(ExtrasCmd::Open(fix.clone()), &mut acts, &ctx);
    let ops: Vec<Vec<String>> = acts
        .iter()
        .filter_map(|a| match a {
            Action::Ops { ops, .. } => ops.first().cloned(),
            _ => None,
        })
        .collect();
    assert_eq!(
        ops,
        [
            [
                "push",
                "--end-of-options",
                "origin",
                "refs/notes/commits:refs/notes/commits"
            ],
            [
                "fetch",
                "--end-of-options",
                "origin",
                "refs/notes/commits:refs/notes/commits"
            ],
        ]
    );
    assert!(matches!(acts.last(), Some(Action::SelectHash(h)) if *h == fix));
    let _ = std::fs::remove_dir_all(&dir);
}

/// What was asked for a commit no longer shown doesn't land on the one shown, and a stash
/// shows none of the sections.
#[test]
fn answers_for_another_commit_are_dropped() {
    let dir = repo("stale");
    let first = commit(&dir, "first");
    let second = commit(&dir, &format!("mentions {}", &first[..7]));
    let ctx = egui::Context::default();
    assert!(git::run(Some(&dir), &["tag", "v2"]).is_ok());
    assert!(git::write_note(&dir, "commits", &first, "on the first").is_ok());
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    // The first's reads start; the second is selected while they run.
    tab.load_extras(&first, &ctx);
    std::thread::sleep(super::SETTLE);
    tab.extras_tick(&ctx);
    assert_eq!(tab.extras.jobs.running, [true; 4]);
    tab.load_extras(&second, &ctx);
    // Every answer for the first comes in, before the second's reads start: none is shown.
    for _ in 0..400 {
        tab.poll(&ctx);
        if tab.extras.jobs.running == [false; 4] {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(tab.extras.jobs.running, [false; 4]);
    let e = &tab.extras;
    assert!(e.signature.is_none() && e.contained.is_none() && e.backlinks.is_none());
    assert!(e.notes.note.is_none());
    // Then the second's own.
    std::thread::sleep(super::SETTLE);
    wait(&mut tab, &ctx, all_read);
    assert_eq!(tab.extras.hash.as_deref(), Some(second.as_str()));
    // The first has a backlink (the second), a note, and v2~1; the second none, none, v2.
    assert!(matches!(&tab.extras.backlinks, Some(Ok((links, _))) if links.is_empty()));
    assert_eq!(note_text(&tab.extras), Some(None));
    assert!(matches!(&tab.extras.contained, Some(Ok(c)) if c.describe.as_deref() == Some("v2")));

    tab.stash_view = Some(0);
    tab.load_extras(&first, &ctx);
    assert_eq!(tab.extras.hash, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The sections draw, folded and opened, while read and once read, with a note being edited.
#[test]
fn the_sections_draw_in_every_state() {
    let dir = repo("draw");
    let target = commit(&dir, "target");
    commit(&dir, &format!("This reverts commit {target}."));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.load_extras(&target, &ctx);
    let remotes = ["origin".to_owned()];
    let draw = |extras: &mut Extras, open: bool| {
        let mut cmds = Vec::new();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                for title in ["Signature", "Contained in", "Notes", "Backlinks"] {
                    let id = egui::Id::new(("commit-extras-open", title));
                    ui.data_mut(|d| d.insert_persisted(id, open));
                }
                extras.ui(ui, &target, &remotes, true, &mut cmds);
            })
            .drop_without_applying_deltas();
        }
        cmds
    };
    // Still being read.
    assert!(draw(&mut tab.extras, true).is_empty());
    wait(&mut tab, &ctx, all_read);
    assert!(draw(&mut tab.extras, false).is_empty());
    assert!(draw(&mut tab.extras, true).is_empty());
    tab.extras.notes.draft = Some("a draft\nwith lines".repeat(500));
    assert!(draw(&mut tab.extras, true).is_empty());
    assert!(tab.extras.notes.draft.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Going through commits quickly reads nothing for those passed on the way, and at most one
/// read of each kind runs at a time; the answers are those of the commit the selection rests
/// on.
#[test]
fn passing_through_commits_starts_one_read_of_each_kind() {
    let dir = repo("debounce");
    let hashes: Vec<String> = (0..20).map(|n| commit(&dir, &format!("c{n}"))).collect();
    // Something only the first has, to tell its answers from the last one's.
    assert!(git::write_note(&dir, "commits", &hashes[0], "only on the first").is_ok());
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    for hash in &hashes {
        tab.load_extras(hash, &ctx);
        tab.extras_tick(&ctx);
    }
    assert_eq!(tab.extras.jobs.running, [false; 4], "read before resting");
    let last = hashes.last().cloned().unwrap_or_default();
    // Rests long enough: one of each starts.
    std::thread::sleep(super::SETTLE);
    tab.extras_tick(&ctx);
    assert_eq!(tab.extras.jobs.running, [true; 4]);
    // Another commit meanwhile: queued behind, the older answers dropped.
    tab.load_extras(&hashes[0], &ctx);
    std::thread::sleep(super::SETTLE);
    wait(&mut tab, &ctx, |e| {
        all_read(e) && e.jobs.running == [false; 4]
    });
    assert_eq!(tab.extras.hash.as_deref(), Some(hashes[0].as_str()));
    assert!(matches!(&tab.extras.backlinks, Some(Ok((l, _))) if l.is_empty()));
    assert_eq!(note_text(&tab.extras), Some(Some("only on the first\n")));
    assert_ne!(last, hashes[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The changes row forgets the commit (nothing is read again for it on reloads), and a note
/// being written comes back with its commit, counted as unfinished work meanwhile.
#[test]
fn a_note_being_written_survives_selecting_elsewhere() {
    let dir = repo("draft");
    let first = commit(&dir, "first");
    let second = commit(&dir, "second");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.load_extras(&first, &ctx);
    std::thread::sleep(super::SETTLE);
    wait(&mut tab, &ctx, all_read);
    tab.extras.notes.draft = Some("half written".to_owned());
    // Switching namespaces is refused while writing.
    let mut acts = Vec::new();
    tab.extras_cmd(
        ExtrasCmd::PickNamespace("other".to_owned()),
        &mut acts,
        &ctx,
    );
    assert_eq!(tab.extras.notes.namespace.as_deref(), Some("commits"));

    tab.load_extras(&second, &ctx);
    assert_eq!(tab.extras.notes.draft, None);
    assert!(tab.note_being_written());
    tab.drop_extras();
    assert_eq!(tab.extras.hash, None);
    tab.extras_after_reload(true, &ctx);
    assert_eq!(tab.extras.jobs.running, [false; 4]);
    tab.load_extras(&first, &ctx);
    assert_eq!(tab.extras.notes.draft.as_deref(), Some("half written"));
    let _ = std::fs::remove_dir_all(&dir);
}
