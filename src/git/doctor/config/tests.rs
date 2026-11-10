use super::*;

#[test]
fn reads_scopes_files_and_bare_keys() {
    let out = "system\0file:/etc/gitconfig\0pull.rebase\nfalse\0\
               global\0file:/home/me/.gitconfig\0pull.rebase\ntrue\0\
               local\0file:.git/config\0core.fsmonitor\0\
               command\0command line:\0core.editor\nvim\0\
               future\0file:x\0a.b\nc\0\
               local\0file:.git/config\0user.name\nZoë Ünïcode\nsecond line\0";
    let all = parse_origins(out);
    assert_eq!(all.len(), 5);
    assert_eq!(all[0].scope, Scope::System);
    assert_eq!(all[0].file.as_deref(), Some("/etc/gitconfig"));
    assert_eq!(all[1].value.as_deref(), Some("true"));
    assert_eq!(all[2].value, None);
    assert_eq!(all[2].shown_value(), "true");
    assert_eq!(all[3].file.as_deref(), Some("command line:"));
    // A value may itself hold a line break: only the first one ends the key.
    assert_eq!(all[4].value.as_deref(), Some("Zoë Ünïcode\nsecond line"));
    let rebase = values_of(&all, "pull.Rebase");
    assert_eq!(rebase.len(), 2);
    assert_eq!(rebase.last().map(|o| o.scope), Some(Scope::Global));
}

#[test]
fn empty_output_is_no_values() {
    assert!(parse_origins("").is_empty());
    assert!(parse_origins("local\0").is_empty());
}

#[test]
fn nothing_credential_related_is_listed() {
    for s in SETTINGS {
        let key = s.key.to_lowercase();
        for word in [
            "credential",
            "token",
            "password",
            "secret",
            "extraheader",
            "auth",
        ] {
            assert!(!key.contains(word), "{key}");
        }
    }
    assert!(find("credential.helper").is_none());
    assert!(find("PULL.REBASE").is_some());
}

#[test]
fn writes_are_checked() {
    assert_eq!(
        set_args(Scope::Local, "pull.rebase", "true"),
        Ok([
            "config",
            "--local",
            "--replace-all",
            "--",
            "pull.rebase",
            "true"
        ]
        .map(str::to_owned)
        .to_vec())
    );
    assert!(set_args(Scope::Command, "pull.rebase", "true").is_err());
    assert!(set_args(Scope::Local, "credential.helper", "store").is_err());
    assert!(set_args(Scope::Local, "user.name", "a\nb").is_err());
    let origin = Origin {
        scope: Scope::Global,
        file: Some("/home/me/.gitconfig".into()),
        key: "pull.rebase".into(),
        value: Some("true".into()),
    };
    assert_eq!(
        unset_args(&origin).map(|a| a[2].clone()),
        Ok("/home/me/.gitconfig".to_owned())
    );
    let command = Origin {
        scope: Scope::Command,
        ..origin
    };
    assert!(unset_args(&command).is_err());
}
