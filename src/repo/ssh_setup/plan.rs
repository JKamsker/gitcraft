//! Inert SSH setup plans. Construction never starts a process or reads key/account data.
use std::path::{Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::remote::{Host, Target, sh_path, sh_quote};
use crate::terminal::ShellSpec;

#[derive(Clone)]
pub(super) enum Machine {
    Local { windows: bool },
    Remote(Target),
}
impl Machine {
    pub fn of(repo: &Path) -> Result<Self, String> {
        if let Some(target) = crate::remote::parse(repo) {
            return Ok(Self::Remote(target));
        }
        Self::unparsed(repo, cfg!(windows))
    }

    /// The exact fallback boundary stays pure so Windows rejection is also exercised
    /// by fixtures on other platforms, without contacting a share or starting WSL.
    fn unparsed(repo: &Path, windows: bool) -> Result<Self, String> {
        let path = repo.to_string_lossy();
        if path.starts_with("ssh:")
            || path.starts_with("wsl:")
            || (windows && reserved_wsl_unc(&path))
            || path
                .split_once("://")
                .is_some_and(|(scheme, _)| !scheme.contains(['/', '\\']))
        {
            return Err(crate::i18n::text("repo-workflows-ssh_setup-unparsed-634afcd2").into());
        }
        Ok(Self::Local { windows })
    }
    pub fn label(&self) -> String {
        match self {
            Self::Local { windows: true } => {
                crate::i18n::text("repo-workflows-ssh_setup-label-53ce73c6").into()
            }
            Self::Local { windows: false } => {
                crate::i18n::text("repo-workflows-ssh_setup-label-27d86821").into()
            }
            Self::Remote(target) => target.host.label(),
        }
    }
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }
    pub fn validate_path(&self, path: &str) -> Result<(), String> {
        let absolute = match self {
            Self::Local { windows: true } => {
                let bytes = path.as_bytes();
                (bytes.len() > 3
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && matches!(bytes[2], b'/' | b'\\'))
                    || path.starts_with("\\\\")
            }
            Self::Local { windows: false } => path.starts_with('/'),
            Self::Remote(_) => path.starts_with('/') || path.starts_with("~/"),
        };
        if !absolute || path.chars().any(char::is_control) || path.len() > 4096 {
            return Err(
                crate::i18n::text("repo-workflows-ssh_setup-validate-path-e922aa6d").into(),
            );
        }
        if path.ends_with(['/', '\\']) || path.ends_with(".pub") {
            return Err(
                crate::i18n::text("repo-workflows-ssh_setup-validate-path-371cfd2c").into(),
            );
        }
        Ok(())
    }
    fn direct(&self, program: &str, args: Vec<String>, label: &str) -> Result<Invocation, String> {
        match self {
            Self::Local { .. } => Ok(Invocation {
                shell: ShellSpec {
                    program: program.into(),
                    args,
                    label: label.into(),
                },
                reserve_folder: None,
            }),
            Self::Remote(_) => {
                Err(crate::i18n::text("repo-workflows-ssh_setup-direct-0b8ceab7").into())
            }
        }
    }
    fn script(&self, script: &str, label: &str) -> Result<Invocation, String> {
        let shell = match self {
            Self::Remote(Target {
                host: Host::Ssh(host),
                ..
            }) => {
                if !crate::remote::valid_ssh_host(host) {
                    return Err(
                        crate::i18n::text("repo-workflows-ssh_setup-script-29797f05").into(),
                    );
                }
                // The login shell sees only portable quoted words and base64 data. The
                // fixed POSIX shell decodes the script; no user shell interpolation occurs.
                let encoded = STANDARD.encode(script);
                let launch =
                    format!("exec sh -c 'eval \"$(printf %s \"$1\" | base64 -d)\"' sh {encoded}");
                ShellSpec {
                    program: "ssh".into(),
                    args: vec![
                        "-t".into(),
                        "-a".into(),
                        "--".into(),
                        crate::remote::ssh_destination(host),
                        launch,
                    ],
                    label: label.into(),
                }
            }
            Self::Remote(Target {
                host: Host::Wsl(distro),
                ..
            }) => {
                if !crate::remote::valid_distro(distro) {
                    return Err(
                        crate::i18n::text("repo-workflows-ssh_setup-script-414dd974").into(),
                    );
                }
                ShellSpec {
                    program: "wsl.exe".into(),
                    args: vec![
                        "-d".into(),
                        distro.clone(),
                        "--exec".into(),
                        "sh".into(),
                        "-c".into(),
                        script.into(),
                    ],
                    label: label.into(),
                }
            }
            Self::Local { .. } => {
                return Err(crate::i18n::text("repo-workflows-ssh_setup-script-6a56625c").into());
            }
        };
        Ok(Invocation {
            shell,
            reserve_folder: None,
        })
    }
}

/// WSL share names are reserved, including when no valid distro follows them.
/// Match a complete server component, rather than a prefix of an ordinary UNC name.
fn reserved_wsl_unc(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let rest = normalized
        .strip_prefix("//?/unc/")
        .or_else(|| normalized.strip_prefix("//./unc/"))
        .or_else(|| normalized.strip_prefix("//"));
    matches!(
        rest.and_then(|rest| rest.split('/').next()),
        Some("wsl.localhost" | "wsl$")
    )
}

pub(super) struct Invocation {
    pub shell: ShellSpec,
    /// The local runner must create this exact new directory using create_dir, never
    /// create_dir_all. Failure, including AlreadyExists, refuses launch. Nothing is removed.
    pub reserve_folder: Option<PathBuf>,
}

pub(super) fn generate(
    machine: &Machine,
    folder: &str,
    title: &str,
) -> Result<(Invocation, String), String> {
    if folder.is_empty()
        || folder.ends_with(['/', '\\'])
        || folder.ends_with("/..")
        || folder.ends_with("/.")
    {
        return Err(crate::i18n::text("repo-workflows-ssh_setup-generate-60bb12f0").into());
    }
    let separator = if matches!(machine, Machine::Local { windows: true }) {
        '\\'
    } else {
        '/'
    };
    let key = format!("{folder}{separator}id_ed25519");
    machine.validate_path(&key)?;
    text(
        title,
        crate::i18n::text("repo-workflows-ssh_setup-generate-b9e8c614"),
    )?;
    let mut plan = if machine.is_local() {
        machine.direct(
            "ssh-keygen",
            vec![
                "-t".into(),
                "ed25519".into(),
                "-f".into(),
                key.clone(),
                "-C".into(),
                title.into(),
            ],
            "Create SSH key",
        )?
    } else {
        let script = format!(
            "set -eu\numask 077\nfolder={}\nmkdir -- \"$folder\"\nexec ssh-keygen -t ed25519 -f \"$folder/id_ed25519\" -C {}\n",
            sh_path(folder),
            sh_quote(title)
        );
        machine.script(&script, "Create SSH key there")?
    };
    if machine.is_local() {
        plan.reserve_folder = Some(PathBuf::from(folder));
    }
    Ok((plan, key))
}

pub(super) fn agent(machine: &Machine, key: &str) -> Result<Invocation, String> {
    machine.validate_path(key)?;
    if machine.is_local() {
        machine.direct(
            "ssh-add",
            vec!["--".into(), key.into()],
            "Add selected SSH key to agent",
        )
    } else {
        machine.script(
            &format!("set -eu\nkey={}\nexec ssh-add -- \"$key\"\n", sh_path(key)),
            "Add SSH key to agent there",
        )
    }
}

pub(super) fn test(machine: &Machine, key: &str, destination: &str) -> Result<Invocation, String> {
    machine.validate_path(key)?;
    if !crate::remote::valid_ssh_host(destination) {
        return Err(crate::i18n::text("repo-workflows-ssh_setup-test-2984ed5d").into());
    }
    let dest = crate::remote::ssh_destination(destination);
    // Ignore configured additive IdentityFile/agent identities so this tests the chosen
    // key. This direct diagnostic does not change the user's SSH configuration.
    let args = vec![
        "-F".into(),
        "none".into(),
        "-T".into(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        "IdentityAgent=none".into(),
        "-i".into(),
        key.into(),
        "--".into(),
        dest.clone(),
    ];
    if machine.is_local() {
        machine.direct("ssh", args, "Test selected SSH key")
    } else {
        machine.script(&format!("set -eu\nkey={}\nexec ssh -F none -T -o IdentitiesOnly=yes -o IdentityAgent=none -i \"$key\" -- {}\n",sh_path(key),sh_quote(&dest)),"Test selected SSH key there")
    }
}

pub(super) fn remote_registration(
    machine: &Machine,
    key: &str,
    host: &str,
    github_user: Option<&str>,
    tea_login: Option<&str>,
    title: &str,
) -> Result<Invocation, String> {
    machine.validate_path(key)?;
    text(
        title,
        crate::i18n::text("repo-workflows-ssh_setup-remote-registration-b9e8c614"),
    )?;
    if machine.is_local() {
        return Err(
            crate::i18n::text("repo-workflows-ssh_setup-remote-registration-bcf7fa5e").into(),
        );
    }
    let script = if let Some(user) = github_user {
        let account = crate::connections::Account {
            host: host.into(),
            user: user.into(),
            config_dir: String::new(),
        };
        account.validate()?;
        if !account.selected() {
            return Err(
                crate::i18n::text("repo-workflows-ssh_setup-remote-registration-6fbacbe7").into(),
            );
        }
        // A named remote account overrides ambient tokens without changing global login.
        // The transient token travels only through a shell variable and command environment.
        format!(
            "set +x\nset -eu\nunset GH_DEBUG GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GH_HOST GH_REPO\n{}host={}\nuser={}\ntoken=$(gh auth token --hostname \"$host\" --user \"$user\" 2>/dev/null) || {{ printf '%s\\n' 'Could not load the named GitHub login.' >&2; exit 1; }}\nexport GH_TOKEN=\"$token\" GH_ENTERPRISE_TOKEN=\"$token\"\nactual=$(gh api --hostname \"$host\" user --jq .login 2>/dev/null)\n[ \"$actual\" = \"$user\" ] || {{ printf '%s\\n' 'Account mismatch; public key was not uploaded.' >&2; exit 1; }}\nif gh api --hostname \"$host\" --method POST user/keys --raw-field {} --raw-field \"key=$public\" >/dev/null 2>&1; then printf '%s\\n' 'Public key registered for the selected GitHub account.'; else printf '%s\\n' 'Registration failed; check key-management permission.' >&2; exit 1; fi\n",
            public_input(key),
            sh_quote(host),
            sh_quote(user),
            sh_quote(&format!("title={title}"))
        )
    } else if let Some(login) = tea_login {
        text(
            login,
            crate::i18n::text("repo-workflows-ssh_setup-remote-registration-af72205a"),
        )?;
        if login.is_empty() || login.starts_with('-') {
            return Err(
                crate::i18n::text("repo-workflows-ssh_setup-remote-registration-09f739fd").into(),
            );
        }
        // tea0.16+ exposes this explicit-login command. Its preview contains metadata
        // only; the user confirms that this alias's URL/account is the intended server.
        format!(
            "set +x\nset -eu\nunset GITEA_SERVER_URL GITEA_SERVER_TOKEN GITEA_SERVER_USER GITEA_SERVER_PASSWORD GITEA_SERVER_OTP\n{}tea login -- {} 2>/dev/null\ntea api --login {} user 2>/dev/null\nprintf '%s' 'Check the login URL/account above. Type UPLOAD to register this public key: '\nIFS= read -r answer\n[ \"$answer\" = UPLOAD ] || exit 1\npublic_file=$(mktemp \"${{TMPDIR:-/tmp}}/gitcraft-public-key.XXXXXXXX\")\ntrap 'rm -f -- \"$public_file\"' EXIT\nprintf '%s\\n' \"$public\" > \"$public_file\"\nif tea ssh-keys add --login {} --title {} -- \"$public_file\" >/dev/null 2>&1; then printf '%s\\n' 'Public key registered for the chosen tea login.'; else printf '%s\\n' 'Registration failed; check tea0.16+ and key-management permission.' >&2; exit 1; fi\n",
            public_input(key),
            sh_quote(login),
            sh_quote(login),
            sh_quote(login),
            sh_quote(title)
        )
    } else {
        return Err(
            crate::i18n::text("repo-workflows-ssh_setup-remote-registration-d31abaa5").into(),
        );
    };
    machine.script(&script, "Register public SSH key there")
}

/// Reduce the chosen .pub companion to validated public fields before any upload.
/// Comments, extra lines and whole-file forwarding never reach the forge command.
fn public_input(key: &str) -> String {
    const CHECK: &str = r#"public=$(head -c 16385 -- "$key.pub" | awk 'NR==1 && NF>=2 && $1 ~ /^(ssh-ed25519|ssh-rsa|ecdsa-sha2-nistp256|ecdsa-sha2-nistp384|ecdsa-sha2-nistp521|sk-ssh-ed25519@openssh.com|sk-ecdsa-sha2-nistp256@openssh.com)$/ && $2 ~ /^[A-Za-z0-9+\/=]+$/ {print $1 " " $2; ok=1; exit} END {if(!ok)exit 1}') || { printf '%s\n' 'A valid OpenSSH .pub companion is required; no key was uploaded.' >&2; exit 1; }
[ "${#public}" -le 16384 ] || exit 1
printf '%s\n' "$public" | ssh-keygen -l -f /dev/stdin >/dev/null 2>&1 || { printf '%s\n' 'Invalid public key; no key was uploaded.' >&2; exit 1; }
"#;
    format!("key={}\n{CHECK}", sh_path(key))
}

pub(super) fn text(value: &str, label: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) || value.len() > 512 {
        Err(crate::i18n::message(
            "repo-workflows-ssh_setup-text-c18fcabe",
            &[("label", (label).into())],
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_generation_is_interactive_and_requires_exclusive_new_folder() -> Result<(), String> {
        for (windows, folder) in [
            (false, "/fixture/key folder ' ü"),
            (true, r"C:\fixture\key folder ' ü"),
        ] {
            let (plan, key) = generate(&Machine::Local { windows }, folder, "fixture title")?;
            assert_eq!(plan.reserve_folder, Some(PathBuf::from(folder)));
            assert_eq!(plan.shell.program, "ssh-keygen");
            assert!(plan.shell.args.contains(&key));
            assert!(!plan.shell.args.iter().any(|a| a == "-N" || a == "-P"));
        }
        Ok(())
    }
    #[test]
    fn remote_scripts_quote_paths_without_local_key_or_credential_routing() -> Result<(), String> {
        let machine = Machine::Remote(Target {
            host: Host::Ssh("work@build:2222".into()),
            path: "/repo".into(),
        });
        let key = "~/key ' $(touch INJECTED) ü/id_ed25519";
        let invocation = agent(&machine, key)?;
        assert_eq!(invocation.shell.program, "ssh");
        assert_eq!(
            &invocation.shell.args[..4],
            &["-t", "-a", "--", "ssh://work@build:2222"]
        );
        let (_, payload) = invocation
            .shell
            .args
            .last()
            .ok_or("No remote script")?
            .rsplit_once(' ')
            .ok_or("No fixed encoded payload")?;
        let script = String::from_utf8(STANDARD.decode(payload).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        assert!(script.contains(&sh_path(key)));
        assert!(!script.contains("kill") && !script.contains("ps "));
        let (create, _) = generate(&machine, "~/new fixture", "fixture")?;
        assert!(create.reserve_folder.is_none());
        Ok(())
    }
    #[test]
    fn chosen_identity_test_ignores_other_identity_files_and_agent_keys() -> Result<(), String> {
        let plan = test(
            &Machine::Local { windows: false },
            "/fixture/key",
            "git@forge.example:2222",
        )?;
        assert_eq!(
            plan.shell.args.last().map(String::as_str),
            Some("ssh://git@forge.example:2222")
        );
        assert!(plan.shell.args.iter().any(|a| a == "IdentityAgent=none"));
        assert_eq!(&plan.shell.args[..3], &["-F", "none", "-T"]);
        for bad in ["-oProxyCommand=fixture", "bad\nhost", "git@host;touch"] {
            assert!(test(&Machine::Local { windows: false }, "/fixture/key", bad).is_err());
        }
        Ok(())
    }
    #[test]
    fn windows_reserved_wsl_unc_never_falls_back_after_parse_failure() {
        for path in [
            r"\\wsl.localhost\-bad\repo",
            r"\\wsl$\\repo",
            r"\\WSL.LOCALHOST\",
            r"\\wsl$",
            r"\\?\UNC\wsl.localhost\-bad\repo",
            r"\\?\uNc\WSL$\\repo",
            r"\\.\UNC\wsl$\-bad\repo",
            "//wsl.localhost/-bad/repo",
            r"\\WsL.LocalHost/-bad\repo",
            "//?/UnC/WSL.LOCALHOST//repo",
        ] {
            assert!(reserved_wsl_unc(path), "{path}");
            let mut operation = false;
            let result = Machine::unparsed(Path::new(path), true).inspect(|_| {
                operation = true;
            });
            assert!(result.is_err(), "{path}");
            assert!(
                !operation,
                "No key/agent/account operation may follow an invalid machine"
            );
        }
    }

    #[test]
    fn ordinary_unc_and_local_paths_keep_their_platform_boundary() -> Result<(), String> {
        for path in [
            r"\\server\share\repo",
            r"\\?\UNC\server\share\repo",
            r"\\.\UNC\server\share\repo",
            r"\\wsl.localhost.example\share\repo",
            r"\\wsl$-backup\share\repo",
            r"C:\work\repo",
            r"\\?\C:\work\repo",
            "/home/me/repo",
        ] {
            assert!(!reserved_wsl_unc(path), "{path}");
            assert!(
                matches!(
                    Machine::unparsed(Path::new(path), true)?,
                    Machine::Local { windows: true }
                ),
                "{path}"
            );
        }
        // On non-Windows systems these spellings remain ordinary local share paths.
        assert!(matches!(
            Machine::unparsed(Path::new(r"\\wsl.localhost\-bad\repo"), false)?,
            Machine::Local { windows: false }
        ));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_valid_wsl_unc_variants_stay_remote() -> Result<(), String> {
        for path in [
            r"\\wsl.localhost\Ubuntu\home\me\repo",
            r"\\wsl$\Ubuntu\home\me\repo",
            r"\\?\UNC\wsl.localhost\Ubuntu\home\me\repo",
            r"\\?\unc\WSL$\Ubuntu\home\me\repo",
            "//wsl.localhost/Ubuntu/home/me/repo",
            r"\\WsL.LocalHost/Ubuntu\home/me\repo",
        ] {
            assert!(
                matches!(Machine::of(Path::new(path))?, Machine::Remote(Target { host: Host::Wsl(distro), path }) if distro=="Ubuntu" && path=="/home/me/repo")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod fluent_plan_tests {
    use super::*;
    #[test]
    fn fluent_partition_ssh_plan_localizes_validation_without_changing_argv() -> Result<(), String>
    {
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
}
