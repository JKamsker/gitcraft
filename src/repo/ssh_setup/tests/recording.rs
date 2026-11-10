use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
fn tool(fixture: &Fixture, name: &str, body: &str) -> Result<(), String> {
    let bin = fixture.0.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let path = bin.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).map_err(|e| e.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())
}
fn script(invocation: &Invocation) -> Result<String, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if invocation.shell.program == "wsl.exe" {
        return invocation
            .shell
            .args
            .last()
            .cloned()
            .ok_or("No WSL script".into());
    }
    let encoded = invocation
        .shell
        .args
        .last()
        .and_then(|s| s.rsplit_once(' '))
        .map(|(_, payload)| payload)
        .ok_or("No encoded SSH script")?;
    String::from_utf8(STANDARD.decode(encoded).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn run(
    fixture: &Fixture,
    invocation: &Invocation,
    input: &[u8],
) -> Result<std::process::Output, String> {
    let mut command = Command::new("/bin/sh");
    let tmp = fixture.0.join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    command
        .env_clear()
        .current_dir(&fixture.0)
        .env("HOME", &fixture.0)
        .env("TMPDIR", tmp)
        .env("FIXTURE_ROOT", &fixture.0)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", fixture.0.join("bin").display()),
        )
        .env("GH_TOKEN", "foreign-fixture-token")
        .env("GITEA_SERVER_TOKEN", "foreign-fixture-token")
        .args(["-c", &script(invocation)?]);
    crate::processes::run(
        command,
        crate::processes::Options {
            input: Some(input.to_vec()),
            timeout: Some(std::time::Duration::from_secs(3)),
            output_limit: 65536,
            stderr_limit: Some(16384),
            ..Default::default()
        },
    )?
    .complete()
}
fn machine() -> Machine {
    Machine::Remote(crate::remote::Target {
        host: crate::remote::Host::Ssh("fixture-machine".into()),
        path: "/fixture/repository".into(),
    })
}
#[test]
fn remote_generation_agent_and_test_use_recording_tools_and_never_overwrite() -> Result<(), String>
{
    let fixture = Fixture::new()?;
    for name in ["ssh-keygen", "ssh-add", "ssh"] {
        tool(
            &fixture,
            name,
            &format!("printf '%s\\n' \"$@\" >> \"$FIXTURE_ROOT/{name}-args\""),
        )?;
    }
    let folder = "~/key '$(touch INJECTED)' ü";
    let (generate, key) = plan::generate(&machine(), folder, "fixture ' $(touch OTHER) title")?;
    assert!(run(&fixture, &generate, b"")?.status.success());
    let created = fixture.0.join("key '$(touch INJECTED)' ü");
    std::fs::write(created.join("sentinel"), b"fixture-original").map_err(|e| e.to_string())?;
    assert!(!run(&fixture, &generate, b"")?.status.success());
    assert_eq!(
        std::fs::read(created.join("sentinel")).map_err(|e| e.to_string())?,
        b"fixture-original"
    );
    assert!(!fixture.0.join("INJECTED").exists() && !fixture.0.join("OTHER").exists());
    assert!(
        run(&fixture, &plan::agent(&machine(), &key)?, b"")?
            .status
            .success()
    );
    let expected = format!(
        "{}{}",
        fixture.0.display(),
        key.strip_prefix('~')
            .ok_or("No home-relative fixture path")?
    );
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("ssh-add-args")).map_err(|e| e.to_string())?,
        format!("--\n{expected}\n")
    );
    assert!(
        run(
            &fixture,
            &plan::test(&machine(), &key, "git@forge.example:2222")?,
            b""
        )?
        .status
        .success()
    );
    assert!(
        std::fs::read_to_string(fixture.0.join("ssh-args"))
            .map_err(|e| e.to_string())?
            .contains("ssh://git@forge.example:2222")
    );
    Ok(())
}
#[test]
fn remote_registration_uploads_only_validated_public_fields_using_named_login() -> Result<(), String>
{
    use base64::{Engine, engine::general_purpose::STANDARD};
    let fixture = Fixture::new()?;
    tool(&fixture, "ssh-keygen", "cat >/dev/null")?;
    tool(
        &fixture,
        "gh",
        r#"case "$1 $2" in
 'auth token') [ -z "${GH_TOKEN:-}" ] || exit 90; printf 'lookup\n' >> "$FIXTURE_ROOT/gh-lookups"; printf fixture-auth-token;;
 'api --hostname')
   [ "${GH_TOKEN:-}" = fixture-auth-token ] && [ "${GH_ENTERPRISE_TOKEN:-}" = fixture-auth-token ] || exit 91
   if [ "$4" = user ]; then printf fixture-user; else printf '%s\n' "$@" > "$FIXTURE_ROOT/gh-upload-args"; fi;;
 *) exit 92;;
esac"#,
    )?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&11u32.to_be_bytes());
    bytes.extend_from_slice(b"ssh-ed25519");
    bytes.extend_from_slice(&32u32.to_be_bytes());
    bytes.extend_from_slice(&[7; 32]);
    let public = format!("ssh-ed25519 {}", STANDARD.encode(bytes));
    std::fs::write(
        fixture.0.join("chosen.pub"),
        format!("{public} fixture-private-comment\n"),
    )
    .map_err(|e| e.to_string())?;
    let invocation = plan::remote_registration(
        &machine(),
        "~/chosen",
        "github.com",
        Some("fixture-user"),
        None,
        "fixture title",
    )?;
    let output = run(&fixture, &invocation, b"")?;
    assert!(output.status.success());
    let arguments =
        std::fs::read_to_string(fixture.0.join("gh-upload-args")).map_err(|e| e.to_string())?;
    assert!(
        arguments
            .lines()
            .any(|line| line == format!("key={public}"))
    );
    assert!(!arguments.contains("fixture-private") && !arguments.contains("fixture-auth-token"));
    std::fs::write(
        fixture.0.join("chosen.pub"),
        b"-----BEGIN OPENSSH PRIVATE KEY-----\nfixture-private\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(!run(&fixture, &invocation, b"")?.status.success());
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("gh-lookups"))
            .map_err(|e| e.to_string())?
            .lines()
            .count(),
        1
    );
    std::fs::write(
        fixture.0.join("chosen.pub"),
        format!("{public} fixture-private-comment\n"),
    )
    .map_err(|e| e.to_string())?;
    tool(
        &fixture,
        "tea",
        r#"[ -z "${GITEA_SERVER_TOKEN:-}" ] || exit 93
case "$1" in
 login) printf 'fixture-login https://forge.example fixture-user\n';;
 api) printf '{"login":"fixture-user"}\n';;
 ssh-keys) for argument in "$@"; do last=$argument; done; cat "$last" > "$FIXTURE_ROOT/tea-public-payload";;
 *) exit 94;;
esac"#,
    )?;
    let invocation = plan::remote_registration(
        &machine(),
        "~/chosen",
        "forge.example",
        None,
        Some("fixture-login"),
        "fixture title",
    )?;
    assert!(!run(&fixture, &invocation, b"CANCEL\n")?.status.success());
    assert!(!fixture.0.join("tea-public-payload").exists());
    assert!(run(&fixture, &invocation, b"UPLOAD\n")?.status.success());
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("tea-public-payload")).map_err(|e| e.to_string())?,
        format!("{public}\n")
    );
    assert_eq!(
        std::fs::read_dir(fixture.0.join("tmp"))
            .map_err(|e| e.to_string())?
            .count(),
        0
    );
    Ok(())
}
