/// Opt-in real-tool validation. All keys, HOME/config and plaintext are generated
/// fixtures; no user credential environment or default keyring is inherited.
#[test]
fn real_sops_and_generated_age_compare_formats_and_key_rotation_offline() -> Result<(), String> {
    use std::process::Command;
    let Some(sops) = std::env::var_os("GITCRAFT_TEST_SOPS").map(PathBuf::from) else {
        eprintln!(
            "Real SOPS fixture not run: set explicit GITCRAFT_TEST_SOPS and GITCRAFT_TEST_AGE_KEYGEN paths"
        );
        return Ok(());
    };
    let age = std::env::var_os("GITCRAFT_TEST_AGE_KEYGEN")
        .map(PathBuf::from)
        .ok_or("Set the explicit fixture age-keygen path")?;
    if !sops.is_absolute() || !age.is_absolute() || !sops.is_file() || !age.is_file() {
        return Err("Real-tool fixture paths must be absolute installed executables".into());
    }
    let repo = Repo::new()?;
    let home = repo.0.join("isolated-home");
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let isolate = |command: &mut Command| {
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("SOPS_DECRYPTION_ORDER", "age")
            .current_dir(&home);
    };
    let execute = |command: Command, input: Option<Vec<u8>>| -> Result<Vec<u8>, String> {
        let output = crate::processes::run_while(
            command,
            crate::processes::Options {
                input,
                timeout: Some(Duration::from_secs(5)),
                output_limit: model::MAX_BYTES,
                stderr_limit: Some(32768),
                ..Default::default()
            },
            &|| true,
        )?
        .complete()?;
        if !output.status.success() {
            return Err("Offline generated-key SOPS/age fixture command failed (private diagnostics suppressed)".into());
        }
        Ok(output.stdout)
    };
    let mut keys = Vec::new();
    for label in ["first", "second"] {
        let key = home.join(format!("fixture-{label}-age-key.txt"));
        let mut create = Command::new(&age);
        isolate(&mut create);
        create.arg("-o").arg(&key);
        execute(create, None)?;
        let mut public = Command::new(&age);
        isolate(&mut public);
        public.arg("-y").arg(&key);
        let recipient =
            String::from_utf8(execute(public, None)?).map_err(|_| "Invalid fixture recipient")?;
        if !recipient.trim().starts_with("age1") {
            return Err("Expected a generated age fixture recipient".into());
        }
        keys.push((key, recipient.trim().to_owned()));
    }
    tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = Some(sops.clone()));
    let encrypt = |format: Format, input: &[u8], recipient: &str| -> Result<String, String> {
        let mut command = Command::new(&sops);
        isolate(&mut command);
        command.args([
            "encrypt",
            "--input-type",
            format.name(),
            "--output-type",
            format.name(),
            "--age",
            recipient,
            "--filename-override",
            format.hint(),
        ]);
        String::from_utf8(execute(command, Some(input.to_vec()))?)
            .map_err(|_| "Encrypted fixture must be UTF-8".into())
    };
    let decrypt = |format: Format, input: &str, key: &std::path::Path| -> Result<String, String> {
        let mut command = tool::command(format);
        isolate(&mut command);
        command.env("SOPS_AGE_KEY_FILE", key);
        tool::plaintext(execute(command, Some(input.as_bytes().to_vec()))?)
    };
    for (format, path, old, new) in [
        (
            Format::Json,
            "fixture.json",
            "{\"secret\":\"fixture-old\"}\n",
            "{\"secret\":\"fixture-new\"}\n",
        ),
        (
            Format::Yaml,
            "fixture.yaml",
            "secret: fixture-old\n",
            "secret: fixture-new\n",
        ),
        (
            Format::Dotenv,
            "fixture.env",
            "SECRET=fixture-old\n",
            "SECRET=fixture-new\n",
        ),
        (
            Format::Ini,
            "fixture.ini",
            "[app]\nsecret=fixture-old\n",
            "[app]\nsecret=fixture-new\n",
        ),
        (
            Format::Binary,
            "fixture.bin",
            "fixture-old\n",
            "fixture-new\n",
        ),
    ] {
        let first = encrypt(format, old.as_bytes(), &keys[0].1)?;
        let edited = encrypt(format, new.as_bytes(), &keys[0].1)?;
        let rotated = encrypt(format, old.as_bytes(), &keys[1].1)?;
        let encoding = if format == Format::Binary {
            JsonEncoding::Binary
        } else {
            JsonEncoding::Ask
        };
        assert_eq!(
            inspect(path, &first)?.map(|e| e.format),
            Some(if format == Format::Binary {
                Format::Json
            } else {
                format
            })
        );
        let changed =
            compare_with_encoding(path, &first, &edited, encoding, &|| true, |kind, bytes| {
                decrypt(kind, bytes, &keys[0].0)
            })?;
        assert!(changed.content_changed);
        assert!(
            changed
                .rows
                .iter()
                .any(|row| row.text.contains("fixture-old"))
        );
        assert!(
            changed
                .rows
                .iter()
                .any(|row| row.text.contains("fixture-new"))
        );
        let rotation =
            compare_with_encoding(path, &first, &rotated, encoding, &|| true, |kind, bytes| {
                decrypt(
                    kind,
                    bytes,
                    if bytes == rotated {
                        &keys[1].0
                    } else {
                        &keys[0].0
                    },
                )
            })?;
        assert!(
            !rotation.content_changed && rotation.recipients_changed && rotation.metadata_changed
        );
    }
    let binary = encrypt(Format::Binary, b"fixture\0binary", &keys[0].1)?;
    assert!(decrypt(Format::Binary, &binary, &keys[0].0).is_err());
    // JSON with just `data` is valid under every filename, including across a rename.
    let data_old = encrypt(Format::Json, br#"{"data":42}"#, &keys[0].1)?;
    let data_new = encrypt(Format::Json, br#"{"data":43}"#, &keys[0].1)?;
    for path in ["encrypted-config", "fixture.yaml", "fixture.enc.json"] {
        let result = compare_with_encoding(
            path,
            &data_old,
            &data_new,
            JsonEncoding::Document,
            &|| true,
            |kind, bytes| decrypt(kind, bytes, &keys[0].0),
        )?;
        assert!(result.content_changed && result.rows.iter().any(|r| r.text.contains("43")));
    }
    let renamed = Repo::new()?;
    renamed.write("before.json", &data_old)?;
    renamed.commit()?;
    renamed.git(&["mv", "--", "before.json", "after.yaml"])?;
    let key = DiffKey::Work(git::StatusFile {
        path: "after.yaml".into(),
        old_path: Some("before.json".into()),
        area: Area::Staged,
        code: 'R',
    });
    let (before, after) = read_sides(&renamed.0, &key)?;
    let result = compare_with_encoding(
        "after.yaml",
        &before,
        &after,
        JsonEncoding::Document,
        &|| true,
        |kind, bytes| decrypt(kind, bytes, &keys[0].0),
    )?;
    assert!(!result.content_changed);
    // Binary envelopes with .json names must use actual bytes, not JSON-escaped data.
    let text_binary = encrypt(Format::Binary, b"actual binary text\n", &keys[0].1)?;
    let result = compare_with_encoding(
        "binary.enc.json",
        "",
        &text_binary,
        JsonEncoding::Binary,
        &|| true,
        |kind, bytes| decrypt(kind, bytes, &keys[0].0),
    )?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].text, "actual binary text");
    for bytes in [
        b"fixture\0binary".as_slice(),
        b"fixture\xffbinary".as_slice(),
    ] {
        let input = encrypt(Format::Binary, bytes, &keys[0].1)?;
        assert!(
            compare_with_encoding(
                "binary.enc.json",
                "",
                &input,
                JsonEncoding::Binary,
                &|| true,
                |kind, bytes| decrypt(kind, bytes, &keys[0].0)
            )
            .is_err()
        );
    }
    assert!(
        compare_with_encoding(
            "binary.enc.json",
            "",
            &binary,
            JsonEncoding::Document,
            &|| true,
            |kind, bytes| decrypt(kind, bytes, &keys[0].0)
        )
        .is_err()
    );
    eprintln!(
        "Real local SOPS generated-age fixture completed: JSON/YAML/dotenv/INI/binary UTF-8, recipient rotation, explicit ambiguous formats, cross-extension rename, binary NUL/non-UTF8 refusal; isolated HOME and explicit fixture keys only"
    );
    Ok(())
}
