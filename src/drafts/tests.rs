//! Tests of the drafts' files, their writer, the session marker and when saves happen.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::keeper::{EVERY, Keeper};
use super::limits::{MAX_FILE, MAX_TEXT};
use super::session::Session;
use super::writer::{Writer, write};
use super::{CommitDraft, LineDraft, PrText, RebaseDraft, RebaseStep, RepoDrafts, limits, store};

/// A fresh, empty folder named after `name`.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-drafts {name} ü-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn commit(summary: &str) -> RepoDrafts {
    RepoDrafts {
        commit: CommitDraft {
            summary: summary.to_owned(),
            body: "Body line one\n\nLine three: ünïcødé 🦀".to_owned(),
            amend: true,
        },
        ..RepoDrafts::default()
    }
}

fn pull_request() -> RepoDrafts {
    let mut drafts = RepoDrafts::default();
    drafts.pull_requests.insert(
        12,
        PrText {
            comment: "Looks good,\nbut see below".to_owned(),
            review: "Two nits".to_owned(),
            line_text: "Off by one?".to_owned(),
            line: Some(LineDraft {
                path: "src/a file.rs".to_owned(),
                line: 40,
                new_side: true,
                head: "abc123".to_owned(),
            }),
        },
    );
    drafts.pull_requests.insert(
        7,
        PrText {
            comment: "Only a reply".to_owned(),
            ..PrText::default()
        },
    );
    drafts
}

fn rebase() -> RepoDrafts {
    let step = |hash: &str, step: &str, message: &str| RebaseStep {
        hash: hash.to_owned(),
        author: "Ann".to_owned(),
        email: "ann@example.com".to_owned(),
        summary: format!("{hash} summary"),
        original: format!("{hash} summary\n\nbody"),
        merge: false,
        step: step.to_owned(),
        message: message.to_owned(),
    };
    RepoDrafts {
        rebase: Some(RebaseDraft {
            onto: "1111".to_owned(),
            onto_label: "main".to_owned(),
            head: "3333".to_owned(),
            entries: vec![
                step("2222", "drop", ""),
                step("3333", "reword", "Better message"),
            ],
        }),
        ..RepoDrafts::default()
    }
}

/// Writes `drafts` as `repo`'s in `folder` and reads them back.
fn round_trip(folder: &Path, repo: &Path, drafts: RepoDrafts) -> RepoDrafts {
    let file = store::file_of(folder, repo);
    let (bytes, cut) = limits::encode(repo, drafts).unwrap_or_default();
    assert!(!cut);
    assert!(store::write_atomic(&file, &bytes).is_ok());
    store::read(&file, repo).unwrap_or_default()
}

#[test]
fn each_kind_of_draft_comes_back_as_it_was_saved() {
    let dir = folder("round-trip");
    let repo = Path::new("/home/me/my repo");
    for drafts in [commit("Fix the thing"), pull_request(), rebase()] {
        assert_eq!(round_trip(&dir, repo, drafts.clone()), drafts);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_repository_on_another_machine_has_a_file_of_its_own() {
    let dir = folder("remote");
    let local = Path::new("/home/me/app");
    let remote = Path::new("ssh://me@box/home/me/app");
    let (a, b) = (commit("local"), commit("remote"));
    round_trip(&dir, local, a.clone());
    round_trip(&dir, remote, b.clone());
    assert_ne!(store::file_of(&dir, local), store::file_of(&dir, remote));
    assert_eq!(store::read(&store::file_of(&dir, local), local), Ok(a));
    assert_eq!(store::read(&store::file_of(&dir, remote), remote), Ok(b));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_name_is_safe_whatever_the_folder_is_called() {
    let dir = Path::new("/d");
    let file = store::file_of(dir, Path::new(r"C:\Users\me\wéird name: <a|b>?"));
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = name.unwrap_or_default();
    assert!(name.ends_with(".json"));
    assert!(
        !name.contains([' ', ':', '<', '>', '|', '?', '\\', '/']),
        "{name}"
    );
    // Nothing to name it after: the hash alone.
    let root = store::file_of(dir, Path::new("/"));
    assert!(root.file_name().is_some());
}

#[test]
fn a_write_cut_short_leaves_the_last_version() {
    let dir = folder("interrupted");
    let repo = Path::new("/r");
    let file = store::file_of(&dir, repo);
    let first = commit("First");
    round_trip(&dir, repo, first.clone());

    // The process dies after writing the new version, before renaming it over the old one…
    let (bytes, _) = limits::encode(repo, commit("Second")).unwrap_or_default();
    let temp = store::write_temp(&file, &bytes).unwrap_or_default();
    assert!(temp.exists());
    // …or halfway through writing it.
    let half = store::write_temp(&file, &bytes[..bytes.len() / 2]).unwrap_or_default();
    assert_eq!(store::read(&file, repo), Ok(first));

    // What a crash left is cleared once it is old enough not to be another run's.
    let old = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    for leftover in [&temp, &half] {
        let set = std::fs::File::options()
            .write(true)
            .open(leftover)
            .and_then(|f| f.set_modified(old));
        assert!(set.is_ok());
    }
    store::tidy(&dir);
    assert!(!temp.exists() && !half.exists());
    assert!(file.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recent_temporary_file_is_left_for_the_run_writing_it() {
    let dir = folder("recent-temp");
    let file = store::file_of(&dir, Path::new("/r"));
    let temp = store::write_temp(&file, b"{}").unwrap_or_default();
    store::tidy(&dir);
    assert!(temp.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_or_foreign_file_is_not_taken_for_drafts() {
    let dir = folder("damaged");
    let repo = Path::new("/r");
    let file = store::file_of(&dir, repo);
    assert!(std::fs::write(&file, b"{\"format\":1,\"repo\":").is_ok());
    let damaged = store::read(&file, repo);
    assert!(
        damaged
            .as_ref()
            .is_err_and(|e| e.contains("damaged") && e.contains("moved to")),
        "{damaged:?}"
    );
    // Set aside, not lost: the next save can't replace it.
    let mut aside = file.as_os_str().to_owned();
    aside.push(".damaged");
    assert!(PathBuf::from(aside).exists());
    assert!(!file.exists());

    // Another repository's drafts in this file (the same hash) are not this one's.
    let (bytes, _) = limits::encode(Path::new("/other"), commit("Other")).unwrap_or_default();
    assert!(store::write_atomic(&file, &bytes).is_ok());
    assert_eq!(store::read(&file, repo), Ok(RepoDrafts::default()));

    // Nor are those of a later version of the format.
    let later = String::from_utf8_lossy(&bytes).replace("\"format\":1", "\"format\":99");
    assert!(std::fs::write(&file, later).is_ok());
    assert_eq!(
        store::read(&file, Path::new("/other")),
        Ok(RepoDrafts::default())
    );

    // No file: no drafts, and no error.
    assert_eq!(
        store::read(&dir.join("none.json"), repo),
        Ok(RepoDrafts::default())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_too_large_is_not_read() {
    let dir = folder("too-large");
    let repo = Path::new("/r");
    let file = store::file_of(&dir, repo);
    assert!(std::fs::write(&file, vec![b' '; MAX_FILE + 1]).is_ok());
    assert!(store::read(&file, repo).is_err_and(|e| e.contains("larger")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_huge_message_is_kept_shortened_and_said_so_once() {
    let dir = folder("huge");
    let repo = Path::new("/r");
    let huge = commit(&"ä".repeat(MAX_TEXT + 10));
    let writer = Writer::new(Some(dir.clone()));
    writer.save(repo, huge.clone());
    writer.flush(Duration::from_secs(10));
    let said = writer.take_notice();
    assert!(
        said.as_ref()
            .is_some_and(|n| !n.error && n.text.contains("too long")),
        "{said:?}"
    );
    let mut longer = huge;
    longer.commit.body.push('!');
    writer.save(repo, longer);
    writer.flush(Duration::from_secs(10));
    assert_eq!(writer.take_notice(), None, "said once");
    let back = store::read(&store::file_of(&dir, repo), repo).unwrap_or_default();
    assert_eq!(back.commit.summary.len(), MAX_TEXT);
    assert!(back.commit.summary.chars().all(|c| c == 'ä'));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_left_to_keep_removes_the_file() {
    let dir = folder("emptied");
    let repo = Path::new("/r");
    assert_eq!(write(&dir, repo, commit("Soon committed")), Ok(false));
    let file = store::file_of(&dir, repo);
    assert!(file.exists());
    // Whitespace alone is nothing worth keeping.
    let blank = RepoDrafts {
        commit: CommitDraft {
            summary: "  ".to_owned(),
            body: "\n".to_owned(),
            amend: false,
        },
        ..RepoDrafts::default()
    };
    assert!(blank.is_empty());
    assert!(write(&dir, repo, RepoDrafts::default()).is_ok());
    assert!(!file.exists());
    // Removing what isn't there is no error.
    assert!(write(&dir, repo, RepoDrafts::default()).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_writer_keeps_two_repositories_apart_and_on_disk() {
    let dir = folder("writer");
    let (a, b) = (Path::new("/work/a"), Path::new("/work/b"));
    let writer = Writer::new(Some(dir.clone()));
    writer.save(a, commit("In a"));
    writer.save(b, pull_request());
    writer.flush(Duration::from_secs(10));
    assert_eq!(writer.load(a), Ok(commit("In a")));
    assert_eq!(writer.take_notice(), None);

    // A new run reads them from the files.
    let later = Writer::new(Some(dir.clone()));
    assert_eq!(later.load(a), Ok(commit("In a")));
    assert_eq!(later.load(b), Ok(pull_request()));

    // Used up in one, the other's stay.
    later.save(a, RepoDrafts::default());
    later.flush(Duration::from_secs(10));
    assert!(!store::file_of(&dir, a).exists());
    assert_eq!(Writer::new(Some(dir.clone())).load(b), Ok(pull_request()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tab_opened_again_gets_what_was_typed_before_it_is_on_disk() {
    let dir = folder("latest");
    let repo = Path::new("/r");
    let writer = Writer::new(Some(dir.clone()));
    writer.save(repo, commit("Typed just now"));
    // Asked before the thread wrote anything.
    assert_eq!(writer.load(repo), Ok(commit("Typed just now")));
    writer.flush(Duration::from_secs(10));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_writer_that_cannot_write_says_why() {
    let dir = folder("unwritable");
    // The drafts folder is a file: nothing can go in it.
    let blocked = dir.join("drafts");
    assert!(std::fs::write(&blocked, b"").is_ok());
    let writer = Writer::new(Some(blocked));
    writer.save(Path::new("/my repo"), commit("Lost?"));
    writer.flush(Duration::from_secs(10));
    let said = writer.take_notice();
    assert!(
        said.as_ref().is_some_and(|n| n.error
            && n.text
                .starts_with("Couldn't save what you typed in my repo")),
        "{said:?}"
    );
    // Typing on, failing the same way: said once, not at every save.
    writer.save(Path::new("/my repo"), commit("Lost? Still"));
    writer.flush(Duration::from_secs(10));
    assert_eq!(writer.take_notice(), None);
    // The failed drafts are tried again when given again (at exit, say).
    writer.save(Path::new("/my repo"), commit("Lost? Still"));
    writer.flush(Duration::from_secs(10));
    let file = store::file_of(&dir.join("drafts"), Path::new("/my repo"));
    assert!(!file.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_is_tried_again_with_the_same_drafts() {
    let dir = folder("retry");
    let blocked = dir.join("drafts");
    assert!(std::fs::write(&blocked, b"").is_ok());
    let writer = Writer::new(Some(blocked.clone()));
    let repo = Path::new("/r");
    writer.save(repo, commit("Retry me"));
    writer.flush(Duration::from_secs(10));
    // The disk has room again.
    assert!(std::fs::remove_file(&blocked).is_ok());
    writer.save(repo, commit("Retry me"));
    writer.flush(Duration::from_secs(10));
    let file = store::file_of(&blocked, repo);
    assert_eq!(store::read(&file, repo), Ok(commit("Retry me")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_of_another_version_is_left_alone_while_nothing_is_typed() {
    let dir = folder("other-version");
    let repo = Path::new("/r");
    let file = store::file_of(&dir, repo);
    let (bytes, _) = limits::encode(repo, commit("From a newer gitcraft")).unwrap_or_default();
    let later = String::from_utf8_lossy(&bytes).replace("\"format\":1", "\"format\":99");
    assert!(std::fs::write(&file, &later).is_ok());
    let writer = Writer::new(Some(dir.clone()));
    // The tab starts empty and saves that: the file stays.
    assert_eq!(writer.load(repo), Ok(RepoDrafts::default()));
    writer.save(repo, RepoDrafts::default());
    writer.flush(Duration::from_secs(10));
    assert_eq!(std::fs::read_to_string(&file).ok(), Some(later));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_cannot_be_read_is_not_removed_by_the_empty_tab() {
    let dir = folder("unreadable");
    let repo = Path::new("/r");
    let file = store::file_of(&dir, repo);
    // A folder where the file should be: reading it fails, as a locked file would.
    assert!(std::fs::create_dir_all(file.join("x")).is_ok());
    let writer = Writer::new(Some(dir.clone()));
    assert!(writer.load(repo).is_err());
    writer.save(repo, RepoDrafts::default());
    writer.flush(Duration::from_secs(10));
    assert!(file.join("x").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_marker_being_made_is_not_taken_for_a_crash() {
    let dir = folder("starting");
    // What another run left halfway through making its marker.
    assert!(std::fs::write(dir.join("1-2.starting"), b"").is_ok());
    let mut session = Session::start_in(&dir);
    assert!(!session.unclean);
    assert!(!dir.join("1-2.starting").exists());
    session.end();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_data_folder_nothing_is_kept_and_nothing_fails() {
    let writer = Writer::new(None);
    writer.save(Path::new("/r"), commit("Nowhere to go"));
    writer.flush(Duration::from_secs(1));
    assert_eq!(writer.load(Path::new("/r")), Ok(RepoDrafts::default()));
    assert_eq!(writer.take_notice(), None);
}

#[test]
fn a_run_that_did_not_close_is_noticed_once() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD_FOLDER: &str = "GITCRAFT_TEST_UNCLOSED_SESSION_FOLDER";
    if let Some(dir) = std::env::var_os(CHILD_FOLDER) {
        let first = Session::start_in(Path::new(&dir));
        assert!(!first.unclean);
        // Leave the marker without a normal close.
        drop(first);
        return Ok(());
    }
    let dir = folder("sessions");
    // Other tests can fork while a marker is locked and temporarily inherit its descriptor.
    // Make the unclosed run in a single-test child and wait for its actual process exit, so
    // unrelated test workers cannot keep that run's lock alive after its Session is dropped.
    let child = std::process::Command::new(std::env::current_exe()?)
        .args([
            "drafts::tests::a_run_that_did_not_close_is_noticed_once",
            "--exact",
            "--test-threads=1",
        ])
        .env(CHILD_FOLDER, &dir)
        .output()?;
    assert!(child.status.success(), "unclosed run failed: {child:?}");
    let mut second = Session::start_in(&dir);
    assert!(second.unclean);
    second.end();
    // Closed normally: the next start is a clean one.
    let mut third = Session::start_in(&dir);
    assert!(!third.unclean);
    third.end();
    let left = std::fs::read_dir(&dir).map(|l| l.count()).unwrap_or(1);
    assert_eq!(left, 0, "the markers of runs closed normally are gone");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn another_gitcraft_running_is_not_taken_for_a_crash() {
    let dir = folder("two-running");
    let mut running = Session::start_in(&dir);
    let mut second = Session::start_in(&dir);
    assert!(!second.unclean);
    // Its marker is still there, for when it ends.
    second.end();
    running.end();
    let mut next = Session::start_in(&dir);
    assert!(!next.unclean);
    next.end();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saves_come_every_few_seconds_on_leaving_the_window_and_when_a_draft_is_used() {
    use crate::repo::DraftShape;
    let start = Instant::now();
    let mut keeper = Keeper::new(start);
    let path = PathBuf::from("/r");
    let typed = vec![(path.clone(), DraftShape::for_test(true, false, 0))];
    assert!(!keeper.due(start, true, &typed));
    assert!(keeper.next_in(start) <= EVERY);
    assert!(keeper.due(start + EVERY, true, &typed));
    keeper.save(start, Vec::new(), typed.clone());

    // The window loses the focus.
    assert!(keeper.due(start, false, &typed));
    // Coming back isn't a reason.
    assert!(!keeper.due(start, true, &typed));

    // The commit is made: the message is gone at once.
    let committed = vec![(path.clone(), DraftShape::for_test(false, false, 0))];
    assert!(keeper.due(start, true, &committed));
    keeper.save(start, Vec::new(), committed.clone());
    assert!(!keeper.due(start, true, &committed));

    // A new draft isn't urgent: the next save takes it.
    let more = vec![(path, DraftShape::for_test(true, true, 2))];
    assert!(!keeper.due(start, true, &more));
}
