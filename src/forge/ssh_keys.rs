//! Local public-key registration with a captured, explicitly named profile account.
//! No global CLI account switch; errors never contain raw tool/HTTP diagnostics.
use super::{Forge, Hosting, http::Api};
use crate::connections::{self, Context};
use serde_json::{Value, json};
pub(crate) mod public_key;
use public_key::PublicKey;
use std::process::Command;
use std::time::Duration;

const FAILED: &str = "backend-public-key-registration-failed";

pub(crate) struct SshKeyAccount {
    owner: String,
    target: Target,
}
enum Target {
    Github { host: String, command: Box<Command> },
    Gitea(Box<Api>),
}
impl SshKeyAccount {
    pub fn owner(&self) -> &str {
        &self.owner
    }
    pub fn upload(
        self,
        title: &str,
        public_key: &PublicKey,
        active: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if !active() {
            return Err("Public-key upload cancelled before sending.".into());
        }
        if title.len() > 512 || title.chars().any(char::is_control) {
            return Err(crate::i18n::text(
                "backend-choose-a-public-key-title-within-512-bytes-without-control-charac-a2bdb7",
            )
            .into());
        }
        let body = json!({"title":title,"key":public_key.as_str()});
        match self.target {
            Target::Github { host, mut command } => {
                command.args([
                    "api",
                    "--hostname",
                    &host,
                    "--method",
                    "POST",
                    "user/keys",
                    "--input",
                    "-",
                ]);
                execute(*command, Some(&body), active)?;
            }
            Target::Gitea(api) => {
                api.send("POST", "user/keys", Some(&body))
                    .map_err(|_| crate::i18n::text(FAILED).to_owned())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn prepare(
    hosting: &Hosting,
    account: &str,
    active: &dyn Fn() -> bool,
) -> Result<SshKeyAccount, String> {
    if account.trim().is_empty() {
        return Err(crate::i18n::text(
            "backend-choose-the-existing-github-username-or-tea-login-alias-before-che-ac8c75",
        )
        .into());
    }
    if !hosting.web.starts_with("https://") || hosting.web.contains(['\n', '\r']) {
        return Err(crate::i18n::text(
            "backend-ssh-public-key-registration-requires-a-known-https-forge-server-5c786b",
        )
        .into());
    }
    let mut context = connections::current();
    match hosting.forge {
        Forge::GitHub => {
            context.choices.github.host = hosting.host.clone();
            context.choices.github.user = account.into();
            context.choices.github.validate()?;
            let command = github_command(&context, &hosting.host)?;
            let mut check = duplicate_command(&command);
            check.args(["api", "--hostname", &hosting.host, "user"]);
            let answer = execute(check, None, active)?;
            let owner = answer
                .get("login")
                .and_then(Value::as_str)
                .filter(|owner| owner.eq_ignore_ascii_case(account))
                .ok_or(crate::i18n::text(
                    "backend-the-github-account-did-not-match-the-selected-username-94af31",
                ))?
                .to_owned();
            Ok(SshKeyAccount {
                owner,
                target: Target::Github {
                    host: hosting.host.clone(),
                    command: Box::new(command),
                },
            })
        }
        Forge::Forgejo | Forge::Gitea => {
            context.choices.tea_login = account.into();
            context.choices.tea_host = hosting
                .web
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .into();
            context.choices.validate()?;
            let api = context.run(|| super::gitea::ssh_key_api(hosting));
            if !api.logged_in() {
                return Err(crate::i18n::text(
                    "backend-no-existing-tea-login-for-this-https-server-was-found-06ceba",
                )
                .into());
            }
            if !active() {
                return Err("Account check cancelled.".into());
            }
            let answer = api
                .get("user")
                .map_err(|_| crate::i18n::text(FAILED).to_owned())?;
            let owner = answer
                .get("login")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                .ok_or(crate::i18n::text(
                    "backend-the-forge-did-not-identify-its-current-account-c0cbf8",
                ))?
                .to_owned();
            Ok(SshKeyAccount {
                owner,
                target: Target::Gitea(Box::new(api)),
            })
        }
        Forge::GitLab | Forge::Azure | Forge::Bitbucket => Err(crate::i18n::text(
            "backend-this-ssh-helper-registers-keys-on-github-forgejo-and-gitea-7078f5",
        )
        .into()),
    }
}

/// Capture the exact token once. The owner probe and eventual mutation use copies of
/// the same command environment, rather than resolving a possibly changed login again.
fn github_command(context: &Context, host: &str) -> Result<Command, String> {
    let mut command = Command::new(connections::program("gh"));
    crate::shell_path::apply(&mut command);
    context.run(|| connections::configure_cli(&mut command, "gh", host))?;
    command
        .current_dir(std::env::temp_dir())
        .env_remove("GH_DEBUG")
        .env_remove("GH_FORCE_TTY")
        .env_remove("GH_REPO")
        .env("GH_PROMPT_DISABLED", "1");
    Ok(command)
}
fn duplicate_command(command: &Command) -> Command {
    let mut copy = Command::new(command.get_program());
    if let Some(cwd) = command.get_current_dir() {
        copy.current_dir(cwd);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            copy.env(key, value);
        } else {
            copy.env_remove(key);
        }
    }
    copy
}
fn execute(
    command: Command,
    body: Option<&Value>,
    active: &dyn Fn() -> bool,
) -> Result<Value, String> {
    if !active() {
        return Err("Account action cancelled.".into());
    }
    let output = crate::processes::run(
        command,
        crate::processes::Options {
            input: body.map(|value| value.to_string().into_bytes()),
            timeout: Some(Duration::from_secs(20)),
            output_limit: 32768,
            stderr_limit: Some(16384),
            ..Default::default()
        },
    )
    .and_then(|out| out.complete())
    .map_err(|_| crate::i18n::text(FAILED).to_owned())?;
    if !output.status.success() {
        return Err(crate::i18n::text(FAILED).into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| crate::i18n::text(FAILED).to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "gitcraft-ssh-account-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).map_err(|e| e.to_string())?;
            let program = path.join("gh-record-only");
            let base = crate::remote::sh_quote(&path.to_string_lossy());
            std::fs::write(&program,format!(r#"#!/bin/sh
set -eu
base={base}
case "$1 $2" in
 'auth token')
   [ "$3" = --hostname ] && [ "$4" = github.com ] && [ "$5" = --user ] && [ "$6" = fixture-user ] || exit 81
   printf 'lookup\n' >> "$base/lookups"
   if [ -f "$base/changed" ]; then printf fixture-token-new; else printf fixture-token-old; fi;;
 'api --hostname')
   [ "$3" = github.com ] || exit 82
   if [ "$4" = user ]; then
     if [ -f "$base/wrong-owner" ]; then printf '{{"login":"someone-else"}}'; else printf '{{"login":"fixture-user"}}'; fi
   else
     [ "$4 $5 $6 $7 $8" = '--method POST user/keys --input -' ] || exit 83
     [ "${{GH_TOKEN:-}}" = fixture-token-old ] || exit 84
     if [ -f "$base/fail-post" ]; then printf fixture-private-diagnostic >&2; exit 85; fi
     cat > "$base/public-payload"
     printf '{{}}'
   fi;;
 *) exit 86;;
esac
"#)).map_err(|e|e.to_string())?;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            connections::TEST_PROGRAMS.with(|map| map.borrow_mut().insert("gh".into(), program));
            Ok(Self(path))
        }
        fn hosting() -> Hosting {
            Hosting {
                forge: Forge::GitHub,
                host: "github.com".into(),
                path: String::new(),
                web: "https://github.com".into(),
            }
        }
        fn public(&self) -> Result<PublicKey, String> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&11u32.to_be_bytes());
            bytes.extend_from_slice(b"ssh-ed25519");
            bytes.extend_from_slice(&32u32.to_be_bytes());
            bytes.extend_from_slice(&[7; 32]);
            let path = self.0.join("fixture.pub");
            std::fs::write(
                &path,
                format!(
                    "ssh-ed25519 {} discarded-private-comment\n",
                    STANDARD.encode(bytes)
                ),
            )
            .map_err(|e| e.to_string())?;
            public_key::read(&path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            connections::TEST_PROGRAMS.with(|map| {
                map.borrow_mut().remove("gh");
            });
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn checked_github_upload_pins_token_across_login_and_profile_changes() -> Result<(), String> {
        Context::default().run(|| {
            let fixture = Fixture::new()?;
            let account = prepare(&Fixture::hosting(), "fixture-user", &|| true)?;
            assert_eq!(account.owner(), "fixture-user");
            std::fs::write(fixture.0.join("changed"), b"fixture").map_err(|e| e.to_string())?;
            let mut other = Context::default();
            other.choices.github.user = "someone-else".into();
            let key = fixture.public()?;
            other.run(|| account.upload("fixture title", &key, &|| true))?;
            let payload: Value = serde_json::from_slice(
                &std::fs::read(fixture.0.join("public-payload")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(payload["title"], "fixture title");
            assert!(
                payload["key"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("ssh-ed25519 ") && !s.contains("discarded"))
            );
            assert_eq!(
                std::fs::read_to_string(fixture.0.join("lookups"))
                    .map_err(|e| e.to_string())?
                    .lines()
                    .count(),
                1
            );
            Ok(())
        })
    }
    #[test]
    fn account_mismatch_cancel_and_diagnostics_never_submit_or_leak_private_input()
    -> Result<(), String> {
        Context::default().run(|| {
            let fixture = Fixture::new()?;
            std::fs::write(fixture.0.join("wrong-owner"), b"fixture").map_err(|e| e.to_string())?;
            assert!(prepare(&Fixture::hosting(), "fixture-user", &|| true).is_err());
            assert!(!fixture.0.join("public-payload").exists());
            std::fs::remove_file(fixture.0.join("wrong-owner")).map_err(|e| e.to_string())?;
            let account = prepare(&Fixture::hosting(), "fixture-user", &|| true)?;
            let key = fixture.public()?;
            assert!(account.upload("fixture", &key, &|| false).is_err());
            assert!(!fixture.0.join("public-payload").exists());
            let account = prepare(&Fixture::hosting(), "fixture-user", &|| true)?;
            std::fs::write(fixture.0.join("fail-post"), b"fixture").map_err(|e| e.to_string())?;
            let error = account
                .upload("fixture", &key, &|| true)
                .err()
                .ok_or("Expected fixture refusal")?;
            assert!(!error.contains("fixture-private"));
            Ok(())
        })
    }
    #[test]
    fn forgejo_and_gitea_registration_posts_only_public_key_to_user_keys() -> Result<(), String> {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        for provider in ["Forgejo", "Gitea"] {
            let fixture = Fixture::new()?;
            let public = fixture.public()?;
            let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let address = listener.local_addr().map_err(|e| e.to_string())?;
            let server = std::thread::spawn(move || -> Result<(String, String, Value), String> {
                let deadline = std::time::Instant::now() + Duration::from_secs(4);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= deadline {
                                return Err("Owned loopback fixture timed out".into());
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| e.to_string())?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| e.to_string())?;
                let mut reader =
                    std::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|e| e.to_string())?;
                let request = line.trim().to_owned();
                let mut length = 0;
                let mut auth = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).map_err(|e| e.to_string())?;
                    if line == "\r\n" {
                        break;
                    }
                    if line.is_empty() {
                        return Err("Incomplete fixture headers".into());
                    }
                    let (key, value) = line.split_once(':').ok_or("Invalid fixture header")?;
                    if key.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse::<usize>().map_err(|e| e.to_string())?;
                    }
                    if key.eq_ignore_ascii_case("authorization") {
                        auth = value.trim().into();
                    }
                }
                if length > 16384 {
                    return Err("Oversized fixture body".into());
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                stream.write_all(b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").map_err(|e|e.to_string())?;
                Ok((
                    request,
                    auth,
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
                ))
            });
            let account = SshKeyAccount {
                owner: provider.into(),
                target: Target::Gitea(Box::new(Api::fixture(
                    &format!("http://{address}"),
                    super::super::http::Login::Token("fixture-only-token".into()),
                ))),
            };
            account.upload("fixture-title", &public, &|| true)?;
            let (request, auth, payload) =
                server.join().map_err(|_| "Loopback fixture stopped")??;
            assert_eq!(request, "POST /api/v1/user/keys HTTP/1.1");
            assert_eq!(auth, "token fixture-only-token");
            assert_eq!(payload["title"], "fixture-title");
            assert!(payload["key"].as_str().is_some_and(
                |key| key.starts_with("ssh-ed25519 ") && !key.contains("discarded-private")
            ));
        }
        Ok(())
    }
}
