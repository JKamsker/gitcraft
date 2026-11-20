use super::*;
use std::io::{Error, ErrorKind};

/// How a recorded OS answers a directory flush.
pub(super) enum Answer {
    Flushed,
    /// Opened, but the filesystem does not implement the flush (ERROR_INVALID_FUNCTION).
    Unsupported,
    OpenFails(Error),
    FlushFails(Error),
}

/// Records each directory and whether it was the one requested; `answer` replies as an OS.
pub(super) fn recorded<'a>(
    seen: &'a mut Vec<(PathBuf, bool)>,
    answer: impl Fn(&Path) -> Answer + 'a,
) -> impl FnMut(&Path, bool) -> Result<Flushed, Failure> + 'a {
    move |dir, requested| {
        seen.push((dir.to_path_buf(), requested));
        match answer(dir) {
            Answer::Flushed => Ok(Flushed::Done),
            Answer::Unsupported => Ok(Flushed::Unsupported(Error::from_raw_os_error(1))),
            Answer::OpenFails(e) => Err(Failure::new(false, e)),
            Answer::FlushFails(e) => Err(Failure::new(true, e)),
        }
    }
}

pub(super) fn paths(seen: &[(PathBuf, bool)]) -> Vec<PathBuf> {
    seen.iter().map(|(p, _)| p.clone()).collect()
}

fn deep() -> PathBuf {
    Path::new("repo")
        .join(".git")
        .join("gitcraft")
        .join("parked")
}

fn denied() -> Answer {
    Answer::OpenFails(Error::from(ErrorKind::PermissionDenied))
}

#[test]
fn only_the_requested_directory_is_flushed_patiently() {
    let mut seen = Vec::new();
    let result = climb_ancestors(
        &deep(),
        &mut Default::default(),
        false,
        recorded(&mut seen, |_| Answer::Flushed),
    );
    assert_eq!(result, Ok(()));
    let requested: Vec<bool> = seen.iter().map(|(_, r)| *r).collect();
    assert_eq!(requested, [true, false, false, false, false]);
}

#[test]
fn the_directory_itself_must_flush_even_when_access_is_denied() {
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        if dir == deep() {
            denied()
        } else {
            Answer::Flushed
        }
    });
    let error = climb_ancestors(&deep(), &mut Default::default(), false, sync)
        .err()
        .unwrap_or_else(|| "flush denial was ignored".into());
    assert!(
        error.contains("parked") && error.contains("open"),
        "{error}"
    );
    assert_eq!(paths(&seen), [deep()]);
}

#[test]
fn an_ancestor_that_cannot_be_opened_ends_the_climb_only_on_windows() {
    // Access denied for both rights (C:\Users), or held without write sharing
    // (%LOCALAPPDATA% during a harness run).
    for n in 0..2 {
        let stop = Path::new("repo").join(".git");
        let mut seen = Vec::new();
        let sync = recorded(&mut seen, |dir| match (dir == stop, n) {
            (true, 0) => denied(),
            (true, _) => Answer::OpenFails(Error::from_raw_os_error(32)),
            (false, _) => Answer::Flushed,
        });
        let result = climb_ancestors(&deep(), &mut Default::default(), false, sync);
        // Nothing above it is flushed: the climb stops right there.
        assert_eq!(paths(&seen).last(), Some(&stop), "case {n}");
        assert_eq!(seen.len(), 3, "case {n}");
        assert_eq!(result.is_ok(), cfg!(windows), "case {n}: {result:?}");
    }
}

#[test]
fn a_flush_refused_after_opening_an_ancestor_is_never_skipped() {
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        if dir == Path::new("repo") {
            Answer::FlushFails(Error::from(ErrorKind::PermissionDenied))
        } else {
            Answer::Flushed
        }
    });
    let error = climb_ancestors(&deep(), &mut Default::default(), false, sync)
        .err()
        .unwrap_or_default();
    assert!(error.starts_with("sync repo: flush"), "{error}");
}

#[test]
fn unsupported_flushes_are_noted_and_the_climb_goes_on() {
    super::super::super::effects::NOTES.with(|n| n.borrow_mut().clear());
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        if dir.starts_with("repo/.git") || dir.starts_with("repo\\.git") {
            Answer::Unsupported
        } else {
            Answer::Flushed
        }
    });
    let result = climb_ancestors(&deep(), &mut Default::default(), false, sync);
    assert_eq!(result, Ok(()));
    assert_eq!(seen.len(), 5, "every ancestor is still attempted");
    let notes = super::super::super::effects::NOTES.with(|n| n.borrow().clone());
    assert_eq!(notes.len(), 3, "{notes:?}");
    assert!(
        notes
            .iter()
            .all(|n| n.starts_with("directory-flush-unsupported"))
    );
}

#[test]
fn any_other_ancestor_failure_is_reported_with_its_path() {
    for kind in [
        ErrorKind::NotFound,
        ErrorKind::Unsupported,
        ErrorKind::Other,
    ] {
        let mut seen = Vec::new();
        let sync = recorded(&mut seen, |dir| {
            if dir == Path::new("repo") {
                Answer::OpenFails(Error::from(kind))
            } else {
                Answer::Flushed
            }
        });
        let error = climb_ancestors(&deep(), &mut Default::default(), false, sync)
            .err()
            .unwrap_or_default();
        assert!(error.starts_with("sync repo: open"), "{kind:?}: {error}");
        assert_eq!(seen.len(), 4, "{kind:?}");
    }
}

#[test]
fn shared_ancestors_flush_once_but_each_requested_directory_stays_strict() {
    let mut done = std::collections::BTreeSet::new();
    let mut seen = Vec::new();
    let a = Path::new("repo").join("src").join("a");
    let b = Path::new("repo").join("src").join("b");
    for dir in [&a, &b, &a] {
        let sync = recorded(&mut seen, |_| Answer::Flushed);
        assert_eq!(climb_ancestors(dir, &mut done, false, sync), Ok(()));
    }
    // a, src, repo, "" then only b: src and above were flushed already, a is not repeated.
    assert_eq!(seen.len(), 5, "{seen:?}");
    assert_eq!(seen[4], (b, true));
    // A denied ancestor is not remembered: requested itself later, it must flush.
    let mut done = std::collections::BTreeSet::new();
    let src = Path::new("repo").join("src");
    let deny = |dir: &Path| {
        if dir == src {
            denied()
        } else {
            Answer::Flushed
        }
    };
    let mut seen = Vec::new();
    let first = climb_ancestors(&a, &mut done, false, recorded(&mut seen, deny));
    assert_eq!(first.is_ok(), cfg!(windows));
    let error = climb_ancestors(&src, &mut done, false, recorded(&mut seen, deny))
        .err()
        .unwrap_or_default();
    assert!(error.contains("src"), "{error}");
}

#[test]
fn real_ancestors_of_a_temporary_archive_flush_up_to_the_first_unwritable_one()
-> Result<(), Box<dyn std::error::Error>> {
    // On Windows an unelevated run meets a directory such as C:\Users on the way up; an
    // elevated run or a Unix run flushes every ancestor. Both must succeed.
    let root = std::env::temp_dir().join(format!("gitcraft-ancestors-{}", std::process::id()));
    let archive = root.join(".git/gitcraft/parked/id");
    std::fs::create_dir_all(&archive)?;
    let result = sync_ancestors(&archive);
    std::fs::remove_dir_all(&root)?;
    result?;
    Ok(())
}

/// A receipt on a filesystem without directory flushes (SMB, `\\wsl$`) is written, read back
/// and noted, instead of failing parking there. See `dir_sync::outcome_tests` for the setup.
#[cfg(windows)]
#[test]
#[ignore = "needs GITCRAFT_TEST_UNFLUSHABLE_DIR on an SMB or 9P path"]
fn receipts_on_a_share_without_directory_flushes_are_kept_and_noted()
-> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var_os("GITCRAFT_TEST_UNFLUSHABLE_DIR")
        .ok_or("set GITCRAFT_TEST_UNFLUSHABLE_DIR")?;
    let archive = Path::new(&base).join(format!("gitcraft-share-journal-{}", std::process::id()));
    std::fs::create_dir(&archive)?;
    super::super::super::effects::NOTES.with(|n| n.borrow_mut().clear());
    let recorded = record(&archive, Phase::Archiving).and_then(|()| latest(&archive));
    let notes = super::super::super::effects::NOTES.with(|n| n.borrow().clone());
    std::fs::remove_dir_all(&archive)?;
    assert_eq!(recorded?, Some(Phase::Archiving));
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("directory-flush-unsupported")),
        "{notes:?}"
    );
    Ok(())
}
