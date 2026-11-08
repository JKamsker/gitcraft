use super::model;
#[test]
fn mime_cover_patch_and_trailers_are_readable() -> Result<(), String> {
    let wire = b"From sender Thu Jan 1 00:00:00 1970\nFrom: =?UTF-8?Q?J=C3=B6nas?= <j@example.test>\nTo: Review <r@example.test>\nSubject: [PATCH v2 1/1] fix\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=fixture\n\n--fixture\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\nReview=C3=A9d\nReviewed-by: Reviewer <r@example.test>\n\n--- a/file\n+++ b/file\n--fixture\nContent-Type: application/octet-stream\nContent-Transfer-Encoding: base64\n\nAQID\n--fixture--\n";
    let mail = model::parse(wire)?;
    assert!(mail.headers.contains("Jönas"));
    assert!(mail.body.contains("Reviewéd"));
    assert!(mail.body.contains("Reviewed-by:"));
    assert!(mail.body.contains("3 decoded bytes"));
    Ok(())
}
#[test]
fn mbox_keeps_messages_and_escaped_from_body() -> Result<(), String> {
    let wire = b"From git@z Thu Jan  1 00:00:00 1970\nSubject: cover\n\n>From body\nFrom unescaped prose stays in cover\nFrom git@z Thu Jan  1 00:00:00 1970\nSubject: patch\n\nSigned-off-by: A <a@example.test>\n";
    let mails = model::mailbox(wire)?;
    assert_eq!(mails.len(), 2);
    assert!(mails[0].body.contains(">From body"));
    assert!(
        mails[0]
            .body
            .contains("From unescaped prose stays in cover")
    );
    assert!(mails[1].body.contains("Signed-off-by"));
    Ok(())
}
#[test]
fn preview_comparison_ignores_only_volatile_headers() -> Result<(), String> {
    let first = model::parse(
        b"Subject: patch\nTo: one@example.test\nDate: yesterday\nMessage-ID: <one>\n\nbody\n",
    )?;
    let regenerated = model::parse(
        b"Subject: patch\nTo: one@example.test\nDate: today\nMessage-ID: <two>\n\nbody\n",
    )?;
    assert_eq!(first, regenerated);
    assert_ne!(
        first,
        model::parse(b"Subject: patch\nTo: wrong@example.test\n\nbody\n")?
    );
    assert_ne!(
        first,
        model::parse(b"Subject: patch\nTo: one@example.test\n\nchanged\n")?
    );
    Ok(())
}
#[test]
fn native_arguments_never_interpolate_source_or_recipients() -> Result<(), String> {
    assert!(model::source("--exec=bad").is_err());
    assert!(model::source("http://inbox.test/id").is_err());
    assert!(model::source("https://user:secret@inbox.test/id").is_err());
    assert!(model::source("<message@example.test>").is_ok());
    let value = "A <a@example.test>, B <b@example.test>";
    let recipients = model::recipients(value, "c@example.test")?;
    assert_eq!(recipients, vec!["--to", value, "--cc", "c@example.test"]);
    assert!(model::recipients("a@example.test\nBcc: stolen@example.test", "").is_err());
    let args = model::command(
        "/venv/bin/python3",
        &["am".into(), "--".into(), "$(touch bad)@example.test".into()],
        false,
    );
    assert!(!args[1].contains("touch bad"));
    assert_eq!(
        args.last().map(String::as_str),
        Some("$(touch bad)@example.test")
    );
    Ok(())
}
#[test]
fn malformed_or_oversized_mail_is_refused() {
    assert!(model::parse(b"From envelope").is_err());
    assert!(model::parse(b"To: a@example.test\n\nno subject").is_err());
    assert!(model::parse(&vec![b'x'; model::MAX + 1]).is_err());
}
#[test]
fn interpreter_and_final_send_gate_do_not_send_stale_mail() -> Result<(), String> {
    let python = if cfg!(windows) {
        "C:/venv/bin/python3"
    } else {
        "/venv/bin/python3"
    };
    assert_eq!(
        model::interpreter(format!("#!{python}\nimport b4").as_bytes())?,
        python
    );
    #[cfg(windows)]
    assert!(model::interpreter(b"#!/venv/bin/python3\n").is_err());
    assert_eq!(model::interpreter(b"#!/usr/bin/env python3\n")?, "python3");
    assert!(model::interpreter(b"#!/bin/sh\n").is_err());
    let shown = vec![model::parse(
        b"Subject: reviewed\nTo: a@example.test\n\npatch\n",
    )?];
    let changed = vec![model::parse(
        b"Subject: reviewed\nTo: b@example.test\n\npatch\n",
    )?];
    let called = std::cell::Cell::new(0);
    assert!(
        model::send_reviewed(&shown, &changed, || {
            called.set(called.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(called.get(), 0);
    model::send_reviewed(&shown, &shown, || {
        called.set(called.get() + 1);
        Ok(())
    })?;
    assert_eq!(called.get(), 1);
    Ok(())
}
#[test]
fn displayed_snapshot_guards_all_authoring_and_apply_inputs() {
    let shown = super::run::Snapshot {
        context: Default::default(),
        path: "/owned/worktree".into(),
        head: "a".repeat(40),
        branch: "topic".into(),
        status: String::new(),
        config: "config-hash".into(),
    };
    assert!(super::run::compare(&shown, &shown).is_ok());
    let mut changed = shown.clone();
    changed.head = "b".repeat(40);
    assert!(super::run::compare(&shown, &changed).is_err());
    changed = shown.clone();
    changed.branch = "other".into();
    assert!(super::run::compare(&shown, &changed).is_err());
    changed = shown.clone();
    changed.context.choices.github.user = "other".into();
    assert!(super::run::compare(&shown, &changed).is_err());
    changed = shown.clone();
    changed.config = "changed".into();
    assert!(super::run::compare(&shown, &changed).is_err());
    changed = shown;
    changed.status = " M tracked".into();
    assert!(super::run::compare(&changed, &changed).is_err());
}
#[test]
fn reroll_normalizes_brackets_and_rejects_url() -> Result<(), String> {
    assert_eq!(
        model::reroll_id("<sent@example.test>")?,
        "sent@example.test"
    );
    assert_eq!(model::reroll_id("sent@example.test")?, "sent@example.test");
    assert!(model::reroll_id("https://lore.kernel.org/all/sent@example.test/").is_err());
    assert!(model::reroll_id("<sent@example.test").is_err());
    assert!(model::reroll_id("sent@example.test>").is_err());
    Ok(())
}
#[test]
fn confirmed_native_entrypoint_is_separate_and_pinned() {
    let args = model::command("/venv/bin/python3", &["send".into()], true);
    assert_eq!(args[3], "/venv/bin/python3");
    assert!(args[1].contains(" -I -c "));
    assert_eq!(args[5], "confirmed-send");
    assert!(args[4].contains("ce4c1158edb292caca0ce4a6087e5edecc1b0371"));
    assert!(args[4].contains("b4.save-maildirs=no"));
    assert!(args[4].contains("check_contract(b4.__VERSION__, sources)"));
    let preview = model::command(
        "/venv/bin/python3",
        &["send".into(), "-o".into(), "/owned/preview".into()],
        false,
    );
    assert_eq!(preview[5], "no-send");
}
