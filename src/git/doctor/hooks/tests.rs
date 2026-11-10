use super::*;

fn scanned(name: &str, executable: bool, head: &[u8]) -> Scanned {
    Scanned {
        name: name.into(),
        folder: false,
        executable,
        head: head.to_vec(),
    }
}

#[test]
fn interpreters_are_read_from_the_shebang() {
    assert_eq!(interpreter(b"#!/bin/sh\necho"), Some("/bin/sh".into()));
    assert_eq!(interpreter(b"#!/usr/bin/env node\n"), Some("node".into()));
    assert_eq!(
        interpreter(b"#!/usr/bin/env -S python3 -u\n"),
        Some("python3".into())
    );
    assert_eq!(interpreter(b"#! /bin/bash\r\n"), Some("/bin/bash".into()));
    assert_eq!(
        interpreter(b"#!/usr/bin/env -S VAR=1 node\n"),
        Some("node".into())
    );
    assert_eq!(
        interpreter(b"#!/usr/bin/env -u X node\n"),
        Some("node".into())
    );
    assert_eq!(interpreter(b"echo hi\n"), None);
    assert_eq!(interpreter(b"#!\n"), None);
    assert_eq!(interpreter(b""), None);
}

#[test]
fn problems_are_found() {
    let ok = check(
        &scanned("pre-commit", true, b"#!/bin/sh\nexit 0\n"),
        &["/bin/sh".into()],
        true,
    );
    assert!(ok.problems.is_empty());
    assert_eq!(ok.runs_on, Some("git commit"));

    let crlf = check(
        &scanned("pre-push", true, b"#!/bin/sh\r\nexit 0\r\n"),
        &["/bin/sh".into()],
        true,
    );
    assert_eq!(crlf.problems, [Problem::CrlfShebang]);
    let lines = check(
        &scanned("pre-push", true, b"#!/bin/sh\nexit 0\r\n"),
        &["/bin/sh".into()],
        true,
    );
    assert_eq!(lines.problems, [Problem::CrlfLines]);

    let off = check(
        &scanned("commit-msg", false, b"#!/bin/sh\n"),
        &["/bin/sh".into()],
        true,
    );
    assert_eq!(off.problems, [Problem::NotExecutable]);
    // Where git doesn't look at the bit (Git for Windows), it isn't a problem.
    assert!(
        check(
            &scanned("commit-msg", false, b"#!/bin/sh\n"),
            &["/bin/sh".into()],
            false
        )
        .problems
        .is_empty()
    );

    let node = check(
        &scanned("pre-commit", true, b"#!/usr/bin/env node\n"),
        &[],
        true,
    );
    assert_eq!(node.problems, [Problem::MissingInterpreter("node".into())]);
    let old = check(
        &scanned("pre-commit.old", true, b"#!/bin/sh\n"),
        &["/bin/sh".into()],
        true,
    );
    assert_eq!(old.problems, [Problem::UnknownName]);
    // Without a #! line git runs it with /bin/sh; Git for Windows can't.
    let sh = check(&scanned("post-merge", true, b"echo hi\n"), &[], true);
    assert!(sh.problems.is_empty());
    let bare = check(&scanned("post-merge", true, b"echo hi\n"), &[], false);
    assert_eq!(bare.problems, [Problem::NoShebang]);
    // A compiled program needs none.
    let elf = check(&scanned("post-merge", true, b"\x7fELF\x02"), &[], false);
    assert!(elf.problems.is_empty());
    // CRLF below the #! line only matters to a shell.
    let node = check(
        &scanned("pre-push", true, b"#!/usr/bin/env node\nx\r\n"),
        &["node".into()],
        true,
    );
    assert!(node.problems.is_empty());
    // A name git never runs: only that is said, whatever else is wrong with it.
    let other = check(&scanned(".gitignore", false, b"*\r\n"), &[], true);
    assert_eq!(other.problems, [Problem::UnknownName]);
    let folder = Scanned {
        folder: true,
        ..scanned("pre-commit", true, b"")
    };
    assert_eq!(check(&folder, &[], true).problems, [Problem::IsFolder]);
}

#[test]
fn the_remote_scan_is_read() {
    let b64 = base64::engine::general_purpose::STANDARD;
    let out = format!(
        "{}\n01\n{}\n{}\n00\n\n{}\n11\n\n{}\n11\n\nbroken\n",
        b64.encode("pré commit"),
        b64.encode(b"#!/bin/sh\n"),
        b64.encode("empty"),
        b64.encode("zz-folder"),
        b64.encode("pre-push"),
    );
    let list = parse_scan(&out);
    // The folder named as a hook is kept, the other one isn't.
    assert_eq!(list.len(), 3);
    assert!(list.iter().any(|s| s.name == "pre-push" && s.folder));
    let named = |n: &str| list.iter().find(|s| s.name == n);
    assert!(named("empty").is_some_and(|s| !s.executable && s.head.is_empty()));
    assert!(named("pré commit").is_some_and(|s| s.executable && s.head == b"#!/bin/sh\n"));
    assert!(parse_scan("").is_empty());
}

#[test]
fn crlf_becomes_lf() {
    assert_eq!(
        crlf_to_lf(b"#!/bin/sh\r\nexit 0\r\n"),
        b"#!/bin/sh\nexit 0\n"
    );
    assert_eq!(crlf_to_lf(b"a\rb\n"), b"a\rb\n");
    assert_eq!(crlf_to_lf(b""), b"");
    assert_eq!(crlf_to_lf(b"\r"), b"\r");
}

#[test]
fn managers_are_recognised() {
    let husky = detect_managers(".husky/_", Some(".husky/_"), &[], "");
    assert_eq!(husky, [Manager::Husky]);
    let lefthook = detect_managers(
        ".git/hooks",
        None,
        &[scanned("pre-commit", true, b"#!/bin/sh\n# lefthook\n")],
        "",
    );
    assert_eq!(lefthook, [Manager::Lefthook]);
    let pre = detect_managers(".git/hooks", None, &[], ".pre-commit-config.yaml\0");
    assert_eq!(pre, [Manager::PreCommit]);
    assert!(detect_managers(".git/hooks", None, &[], "").is_empty());
}
