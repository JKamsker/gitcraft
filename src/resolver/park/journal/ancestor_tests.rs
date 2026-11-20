use super::*;
use std::io::{Error, ErrorKind};

/// Records each flushed directory; `denied` and `failing` answer as an OS would.
fn recorded<'a>(
    seen: &'a mut Vec<PathBuf>,
    answer: impl Fn(&Path) -> Option<ErrorKind> + 'a,
) -> impl FnMut(&Path) -> std::io::Result<()> + 'a {
    move |dir| {
        seen.push(dir.to_path_buf());
        answer(dir).map_or(Ok(()), |kind| Err(Error::from(kind)))
    }
}

fn deep() -> PathBuf {
    Path::new("repo")
        .join(".git")
        .join("gitcraft")
        .join("parked")
}

#[test]
fn the_directory_itself_must_flush_even_when_access_is_denied() {
    let path = deep();
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        (dir == deep()).then_some(ErrorKind::PermissionDenied)
    });
    let error = climb_ancestors(&path, &mut Default::default(), sync)
        .err()
        .unwrap_or_else(|| "flush denial was ignored".into());
    assert!(error.contains("parked"), "{error}");
    assert_eq!(seen, [deep()]);
}

#[test]
fn an_ancestor_without_entry_rights_ends_the_climb_only_on_windows() {
    let path = deep();
    let denied = Path::new("repo").join(".git");
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        (dir == denied).then_some(ErrorKind::PermissionDenied)
    });
    let result = climb_ancestors(&path, &mut Default::default(), sync);
    // Nothing above the denied ancestor is flushed: the climb stops right there.
    assert_eq!(seen.last().map(PathBuf::as_path), Some(denied.as_path()));
    assert_eq!(seen.len(), 3);
    if cfg!(windows) {
        assert_eq!(result, Ok(()));
    } else {
        let error = result.err().unwrap_or_default();
        assert!(error.contains(".git"), "{error}");
    }
}

#[test]
fn an_ancestor_held_without_write_sharing_ends_the_climb_only_on_windows() {
    let path = deep();
    let held = Path::new("repo").to_path_buf();
    let mut seen = Vec::new();
    let sync = |dir: &Path| {
        seen.push(dir.to_path_buf());
        // ERROR_SHARING_VIOLATION, as seen on %LOCALAPPDATA% during the harness run.
        if dir == held {
            Err(Error::from_raw_os_error(32))
        } else {
            Ok(())
        }
    };
    let result = climb_ancestors(&path, &mut Default::default(), sync);
    assert_eq!(seen.len(), 4);
    assert_eq!(result.is_ok(), cfg!(windows), "{result:?}");
    // The directory itself held is still an error.
    let result = climb_ancestors(&held, &mut Default::default(), |_| {
        Err(Error::from_raw_os_error(32))
    });
    assert!(result.is_err());
}

#[test]
fn any_other_ancestor_failure_is_reported_with_its_path() {
    for kind in [
        ErrorKind::NotFound,
        ErrorKind::Unsupported,
        ErrorKind::Other,
    ] {
        let path = deep();
        let mut seen = Vec::new();
        let sync = recorded(&mut seen, |dir| (dir == Path::new("repo")).then_some(kind));
        let error = climb_ancestors(&path, &mut Default::default(), sync)
            .err()
            .unwrap_or_default();
        assert!(error.starts_with("sync repo:"), "{kind:?}: {error}");
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
        assert_eq!(
            climb_ancestors(dir, &mut done, recorded(&mut seen, |_| None)),
            Ok(())
        );
    }
    // a, src, repo, "" then only b: src and above were flushed already, a is not repeated.
    assert_eq!(seen.len(), 5, "{seen:?}");
    assert_eq!(seen[4], b);
    // A denied ancestor is not remembered: requested itself later, it must flush.
    let mut done = std::collections::BTreeSet::new();
    let src = Path::new("repo").join("src");
    let deny = |dir: &Path| (dir == src).then_some(ErrorKind::PermissionDenied);
    let mut seen = Vec::new();
    let first = climb_ancestors(&a, &mut done, recorded(&mut seen, deny));
    assert_eq!(first.is_ok(), cfg!(windows));
    let error = climb_ancestors(&src, &mut done, recorded(&mut seen, deny))
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
