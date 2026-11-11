//! Messages.

use super::*;

/// The error of `r`, which should have failed (`what`).
pub(super) fn failure<T: std::fmt::Debug>(
    r: Result<T, String>,
    what: &str,
) -> Result<String, String> {
    match r {
        Ok(v) => Err(format!("{what} worked: {v:?}")),
        Err(e) => Ok(e),
    }
}

/// The sample `name` (in `samples/`).
pub(super) fn sample(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/git/errors/samples")
        .join(name);
    // A sample missing reads as a message no expectation matches.
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| format!("missing sample {}: {e}", path.display()))
}

#[test]
pub(super) fn dubious_ownership_in_every_wording() {
    let cases = [
        // Captured: a folder owned by another user, with a space and letters outside ASCII.
        ("dubious_2.43.txt", "/home/me/work/dub dir/ünï"),
        ("unsafe_2.35.2.txt", "/home/me/work/repo"),
        ("dubious_2.45_windows.txt", "D:/Projects/My Repo"),
        // A network share: what git says to add, not the folder it names.
        ("dubious_2.40_share.txt", "%(prefix)///server/share/repo"),
    ];
    for (name, path) in cases {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::DubiousOwnership {
                path: path.to_owned()
            }),
            "{name}"
        );
    }
    assert!(super::super::is_dubious_ownership(&sample(
        "dubious_2.43.txt"
    )));
    assert!(!super::super::is_dubious_ownership(&sample(
        "lock_2.43.txt"
    )));
}

#[test]
pub(super) fn dubious_ownership_never_trusts_everything() {
    let err = "fatal: detected dubious ownership in repository at '*'\n\
               To add an exception for this directory, call:\n\n\
               \tgit config --global --add safe.directory '*'";
    assert_eq!(explain(err), None);
    // A quote in the name, as git quotes it for a shell.
    let err = "fatal: detected dubious ownership in repository at '/srv/it's'\n\
               \tgit config --global --add safe.directory '/srv/it'\\''s'";
    assert_eq!(
        explain(err),
        Some(GitProblem::DubiousOwnership {
            path: "/srv/it's".to_owned()
        })
    );
}

#[test]
pub(super) fn locks_of_the_index_and_of_refs() {
    for (name, lock) in [
        ("lock_2.43.txt", "/home/me/work/r/.git/index.lock"),
        ("lock_commit_2.43.txt", "/home/me/work/r/.git/index.lock"),
        ("lock_2.8.txt", "/home/me/work/r/.git/index.lock"),
        (
            "lock_ref_2.43.txt",
            "/home/me/work/r/.git/refs/heads/master.lock",
        ),
        (
            "lock_windows.txt",
            "C:/Users/Me/source/my repo/.git/index.lock",
        ),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::StaleLock {
                lock: lock.to_owned()
            }),
            "{name}"
        );
    }
    // Not a lock git made, or not because it exists.
    assert_eq!(
        explain("fatal: Unable to create '/x/.git/index.lock': Permission denied"),
        None
    );
}

#[test]
pub(super) fn line_endings_warned_and_refused() {
    let lf_to_crlf = |files: &[&str], fatal| GitProblem::LineEndings {
        files: files.iter().map(|f| (*f).to_owned()).collect(),
        to_crlf: true,
        fatal,
    };
    assert_eq!(
        explain(&sample("crlf_warn_2.43.txt")),
        Some(lf_to_crlf(&["lf.txt"], false))
    );
    assert_eq!(
        explain(&sample("crlf_warn_2.36.txt")),
        Some(lf_to_crlf(&["src/main.rs", "README.md"], false))
    );
    assert_eq!(
        explain(&sample("crlf_fatal_2.43.txt")),
        Some(lf_to_crlf(&["lf2.txt"], true))
    );
    assert_eq!(
        explain(&sample("crlf_input_fatal_2.43.txt")),
        Some(GitProblem::LineEndings {
            files: vec!["crlf.txt".to_owned()],
            to_crlf: false,
            fatal: true,
        })
    );
    // Names with spaces, quotes and letters outside ASCII.
    let warn = "warning: in the working copy of 'dir/ä b's.txt', LF will be replaced by CRLF \
                the next time Git touches it";
    assert_eq!(explain(warn), Some(lf_to_crlf(&["dir/ä b's.txt"], false)));
}

#[test]
pub(super) fn line_endings_give_way_to_the_real_error() {
    let err = format!(
        "{}\n{}",
        sample("crlf_warn_2.43.txt"),
        sample("lock_2.43.txt")
    );
    assert!(matches!(explain(&err), Some(GitProblem::StaleLock { .. })));
}

#[test]
pub(super) fn many_line_ending_warnings_are_kept_short() -> Result<(), String> {
    let err: String = (0..5000)
        .map(|i| format!("warning: in the working copy of 'f{i}.txt', LF will be replaced by CRLF the next time Git touches it\n"))
        .collect();
    let Some(GitProblem::LineEndings { files, .. }) = explain(&err) else {
        return Err("not line endings".to_owned());
    };
    assert_eq!(files.len(), super::super::MAX_FILES);
    assert_eq!(files[0], "f0.txt");
    Ok(())
}

#[test]
pub(super) fn names_too_long_here_and_on_windows() {
    for name in [
        "long_checkout_2.43.txt",
        "longpath_windows.txt",
        "longpath_dir_windows.txt",
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::NameTooLong),
            "{name}"
        );
    }
}

#[test]
pub(super) fn certificate_problems_of_every_tls_library() {
    for (name, detail) in [
        (
            "ssl_2.43.txt",
            "server certificate verification failed. CAfile: /root/.ccr/ca-bundle.crt CRLfile: none",
        ),
        (
            "ssl_openssl.txt",
            "SSL certificate problem: unable to get local issuer certificate",
        ),
        (
            "ssl_selfsigned_openssl3.txt",
            "SSL certificate problem: self-signed certificate in certificate chain",
        ),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::Certificate {
                detail: detail.to_owned()
            }),
            "{name}"
        );
    }
    assert!(matches!(
        explain(&sample("ssl_schannel.txt")),
        Some(GitProblem::Certificate { .. })
    ));
}

#[test]
pub(super) fn broken_transfers() {
    for (name, too_large) in [
        ("http413_2.43.txt", true),
        ("rpc_old_2.9.txt", true),
        ("early_2.43.txt", false),
        ("rpc_curl18.txt", false),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::Transfer { too_large }),
            "{name}"
        );
    }
}

#[test]
pub(super) fn histories_shallow_pushes_and_ssh_keys() {
    assert_eq!(
        explain(&sample("unrelated_2.43.txt")),
        Some(GitProblem::UnrelatedHistories)
    );
    assert_eq!(
        explain(&sample("shallow_2.43.txt")),
        Some(GitProblem::ShallowPush)
    );
    assert_eq!(
        explain(&sample("publickey_openssh.txt")),
        Some(GitProblem::SshKeyRefused {
            login: Some("git@github.com".to_owned())
        })
    );
    assert_eq!(
        explain(&sample("publickey_port.txt")),
        Some(GitProblem::SshKeyRefused {
            login: Some("me@git.example.com".to_owned())
        })
    );
}

/// A refused push is the push banner's; an unknown error, nobody's.
#[test]
pub(super) fn others_are_not_explained() {
    assert_eq!(explain(&sample("rejected_nonff_2.43.txt")), None);
    for err in [
        "",
        "fatal: not a git repository (or any of the parent directories): .git",
        "error: pathspec 'x' did not match any file(s) known to git",
        "\u{0}\u{1}\u{fffd}",
    ] {
        assert_eq!(explain(err), None, "{err:?}");
    }
    // Huge output with nothing known in it.
    assert_eq!(explain(&"remote: counting objects\n".repeat(200_000)), None);
}

#[test]
pub(super) fn value_patterns_match_only_the_value() {
    assert_eq!(fix::exact_pattern("/a.b"), r"^/a\.b$");
    assert_eq!(fix::exact_pattern(r"C:\x (1)+[y]"), r"^C:\\x \(1\)\+\[y\]$");
}
