#[test]
fn local_cli_uses_only_ciphertext_stdin_and_fixed_safe_format_argv() -> Result<(), String> {
    let repo = Repo::new()?;
    program(
        &repo,
        r#"
 test "$#" = 7 || exit 11
 test "$1 $2 $3 $4 $5 $6 $7" = 'decrypt --input-type json --output-type json --filename-override gitcraft.json' || exit 12
 input=$(cat)
 case "$input" in *'ENC[cipher1]'*) printf 'fixture-old-private\n';; *'ENC[cipher2]'*) printf 'fixture-new-private\n';; *) exit 13;; esac
"#,
    )?;
    let path = "secret '$(touch INJECTED)' ü.json";
    let old = json("age1fixture", "cipher1");
    let new = json("age1fixture", "cipher2");
    repo.write(path, &old)?;
    repo.commit()?;
    repo.write(path, &new)?;
    let key = work(path, Area::Unstaged, 'M');
    let result = load(&repo.0, &key, JsonEncoding::Ask, &|| true)?;
    assert!(result.content_changed && !result.recipients_changed);
    assert_eq!(
        std::fs::read_to_string(repo.0.join(path)).map_err(|e| e.to_string())?,
        new
    );
    assert_eq!(repo.git(&["show", &format!(":{path}")])?.trim(), old);
    assert!(!repo.0.join("INJECTED").exists());
    let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
    assert!(data.sops && data.partial);
    let hunk = data
        .lines
        .iter()
        .position(|l| l.kind == crate::repo::diff::Kind::Hunk)
        .ok_or("No encrypted hunk")?;
    let patch = crate::repo::diff::partial_patch(
        &data.header,
        &data.lines,
        &crate::repo::diff::hunk_changes(&data.lines, hunk),
        crate::repo::diff::Pick::Stage,
    )
    .ok_or("No encrypted patch")?;
    assert!(patch.contains("ENC[cipher2]"));
    assert!(!patch.contains("fixture-new-private"));
    assert!(
        data.lines
            .iter()
            .all(|line| !line.text.contains("fixture-new-private"))
    );
    assert!(repo.0.read_dir().map_err(|e| e.to_string())?.all(|entry| {
        entry.is_ok_and(|e| e.file_name() != "before" && e.file_name() != "after")
    }));
    Ok(())
}

#[test]
fn plaintext_tool_diagnostics_missing_tools_binary_and_oversized_output_are_redacted()
-> Result<(), String> {
    let repo = Repo::new()?;
    let input = json("age1fixture", "cipher1");
    program(
        &repo,
        "cat >/dev/null; printf 'fixture-private-stdout'; printf 'fixture-private-stderr' >&2; exit 1",
    )?;
    let error = tool::decrypt(Format::Json, &input, &|| true)
        .err()
        .ok_or("Expected tool failure")?;
    assert!(!error.contains("fixture-private"));
    assert!(error.contains("keys/permissions"));
    tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = Some(repo.0.join("not-installed")));
    assert!(
        tool::decrypt(Format::Json, &input, &|| true).is_err_and(|e| e.contains("Install SOPS"))
    );
    program(&repo, "cat >/dev/null; printf '\\377'")?;
    assert!(tool::decrypt(Format::Json, &input, &|| true).is_err_and(|e| e.contains("UTF-8")));
    program(&repo, "cat >/dev/null; head -c 2097153 /dev/zero")?;
    assert!(tool::decrypt(Format::Json, &input, &|| true).is_err_and(|e| e.contains("size limit")));
    program(&repo, "cat >/dev/null; head -c 32769 /dev/zero >&2")?;
    assert!(tool::decrypt(Format::Json, &input, &|| true).is_err_and(|e| e.contains("size limit")));
    Ok(())
}

#[test]
fn cancellation_kills_local_decryption_and_late_descendants_without_disk_output()
-> Result<(), String> {
    let repo = Repo::new()?;
    let marker = repo.0.join("descendant-reached");
    program(
        &repo,
        &format!(
            "cat >/dev/null; (sleep 0.5; printf leaked > {}) & wait",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )?;
    let start = std::time::Instant::now();
    assert!(
        tool::decrypt(Format::Json, &json("age1fixture", "cipher"), &|| start
            .elapsed()
            < Duration::from_millis(100))
        .is_err_and(|e| e.contains("cancelled"))
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(600));
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn changed_encrypted_sides_discard_plaintext_before_returning_a_result() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    repo.write(path, &json("age1fixture", "cipher1"))?;
    repo.commit()?;
    repo.write(path, &json("age1fixture", "cipher2"))?;
    // Modifying the fixture input simulates an edit while the decrypt subprocess runs.
    program(
        &repo,
        &format!(
            "cat >/dev/null; printf '%s' {} > {}; printf 'fixture-private-output'",
            crate::remote::sh_quote(&json("age1fixture", "cipher3")),
            crate::remote::sh_quote(&repo.0.join(path).to_string_lossy())
        ),
    )?;
    let result = load(
        &repo.0,
        &work(path, Area::Unstaged, 'M'),
        JsonEncoding::Ask,
        &|| true,
    );
    assert!(
        result.is_err_and(
            |e| e.contains("changed while decrypting") && !e.contains("fixture-private")
        )
    );
    Ok(())
}
