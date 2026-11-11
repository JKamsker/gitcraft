use super::*;
#[test]
fn recording_collector_uses_fixed_queries_and_excludes_raw_command_and_versions_suffix()
-> Result<(), String> {
    fn collect(flag: &str) -> Result<MachineFacts, String> {
        let mut unexpected = Vec::new();
        let result = collect_facts(Path::new("/synthetic/repo"), |args| {
            Ok(match args {
                ["rev-parse", "--verify", "HEAD"] => format!("{}\n", "a".repeat(40)),
                ["rev-parse", "--verify", "HEAD^{tree}"] => format!("{}\n", "b".repeat(40)),
                ["ls-files", "--stage", "-z"] => format!("100644 {} 0\tfile\0", "c".repeat(40)),
                ["ls-files", "-v", "-z"] => format!("{flag} file\0"),
                ["ls-files", "--eol", "-z"] => "i/lf w/crlf attr/\tfile\0".into(),
                [
                    "status",
                    "--porcelain=v1",
                    "--untracked-files=no",
                    "--ignore-submodules=none",
                ] => String::new(),
                [
                    "-c",
                    "core.filemode=true",
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--summary",
                    "--",
                ] => String::new(),
                ["config", "--default", "(unset)", "--get", key] if SETTINGS.contains(key) => {
                    "secret-config-value\n".into()
                }
                ["config", "--default", "", "--get", "test.default.command"] => {
                    "secret-test-command\n".into()
                }
                ["-c", TOOL_ALIAS, "gitcraft-machine-tools"] => {
                    "git\tgit version 2.50.1 secret-tool-suffix\nenv\tsecret-environment\n".into()
                }
                [
                    "for-each-ref",
                    "--format=%(objectname)",
                    "refs/notes/tests/default",
                ] => String::new(),
                _ => {
                    unexpected.push(format!("{args:?}"));
                    return Err(format!("Unexpected collector command: {args:?}"));
                }
            }
            .into_bytes())
        });
        assert!(
            unexpected.is_empty(),
            "Unexpected collector commands: {unexpected:?}"
        );
        result
    }
    let facts = collect("H")?;
    assert!(facts.clean);
    assert_eq!(facts.files[0].endings, "i/lf w/crlf attr/");
    assert_eq!(facts.versions["git"], "git version 2.50.1");
    assert!(facts.stored_test.is_none());
    let json = serde_json::to_string(&facts).map_err(|error| error.to_string())?;
    assert!(!json.contains("secret-"));
    assert!(!collect("h")?.clean); // assume-unchanged is not a verified checkout.
    assert!(!collect("S")?.clean); // sparse/skip-worktree is not a verified checkout.
    Ok(())
}
fn sample() -> MachineFacts {
    MachineFacts {
        repository: PathBuf::from("/synthetic/repo"),
        machine: "synthetic".into(),
        head: "a".repeat(40),
        tree: "b".repeat(40),
        content_digest: "content".into(),
        clean: true,
        files: Vec::new(),
        case_collisions: Vec::new(),
        mode_changes: String::new(),
        settings: BTreeMap::new(),
        versions: BTreeMap::new(),
        test_command_digest: "command".into(),
        stored_test: None,
    }
}
#[test]
fn recording_content_comparison_never_guesses_dirty_or_incompatible_content() {
    let a = sample();
    let mut b = a.clone();
    b.head = "c".repeat(40); // Different commit identity can preserve tracked content.
    b.settings.insert("core.autocrlf".into(), "true".into());
    assert_eq!(compare_machine_content(&a, &b), ContentComparison::Match);
    b.content_digest = "changed".into();
    assert_eq!(
        compare_machine_content(&a, &b),
        ContentComparison::Different
    );
    b.head = "c".repeat(64);
    assert_eq!(
        compare_machine_content(&a, &b),
        ContentComparison::Incompatible
    );
    b.clean = false;
    assert_eq!(compare_machine_content(&a, &b), ContentComparison::Changed);
}
#[test]
fn recording_index_comparison_keeps_modes_and_case_distinctions() -> Result<(), String> {
    let files = parse_index(
        format!(
            "100755 {} 0\tFile\0100644 {} 0\tfile\0",
            "a".repeat(40),
            "a".repeat(40)
        )
        .as_bytes(),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].mode, "100755");
    assert_eq!(files[1].path, "file");
    assert!(parse_index(b"malformed\0").is_err());
    assert!(link_path("//wsl.localhost/Ubuntu/home/me/repo").is_ok());
    assert!(link_path("ssh://host/path\ncredential").is_err());
    Ok(())
}
#[test]
fn recording_machine_facts_never_admit_credentials_or_unknown_settings() {
    assert_eq!(
        setting_value("core.eol", "secret"),
        "nonstandard value (not collected)"
    );
    assert_eq!(
        setting_value("credential.helper", "true"),
        "nonstandard value (not collected)"
    );
    let versions = parse_versions(
        "env\tSECRET=value\ngit\tgit version 2.50.1 secret=value\nnode\tv22.1.0\npython3\tcredential-secret\ncargo\tcargo 1.95.0\n",
    );
    assert_eq!(versions.len(), 3);
    assert_eq!(versions["git"], "git version 2.50.1");
    assert!(!versions.contains_key("env"));
    assert!(!versions.contains_key("python3"));
    assert!(parse_index(format!("100644 {} 1\tfile\0", "a".repeat(40)).as_bytes()).is_err());
    assert!(link_path("../outside").is_err());
    assert!(link_path("ssh://build/home/me/repo").is_ok());
}
