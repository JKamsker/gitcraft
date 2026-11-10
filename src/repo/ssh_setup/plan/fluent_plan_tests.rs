use super::*;
#[test]
fn fluent_partition_ssh_plan_localizes_validation_without_changing_argv() -> Result<(), String> {
    let machine = Machine::Local { windows: false };
    for lang in ["en-US", "de"] {
        crate::i18n::set_language(lang);
        let (plan, key) = generate(&machine, "/inert/ß key", "Customer's title")?;
        assert_eq!(key, "/inert/ß key/id_ed25519");
        assert_eq!(plan.shell.program, "ssh-keygen");
        assert_eq!(
            plan.shell.args,
            [
                "-t",
                "ed25519",
                "-f",
                "/inert/ß key/id_ed25519",
                "-C",
                "Customer's title"
            ]
        );
        assert_eq!(plan.shell.label, "Create SSH key");
        assert_eq!(plan.reserve_folder, Some(PathBuf::from("/inert/ß key")));
        assert_eq!(
            machine.label(),
            if lang == "de" {
                "Dieser Computer · OpenSSH"
            } else {
                "This computer · OpenSSH"
            }
        );
        let error = machine
            .validate_path("relative.pub")
            .err()
            .ok_or("validation accepted relative path")?;
        assert!(error.contains(if lang == "de" {
            "absoluten Schlüsselpfad"
        } else {
            "absolute key path"
        }));
    }
    Ok(())
}
