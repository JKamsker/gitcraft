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
    pub fn of(repo: &Path) -> Self {
        match crate::remote::parse(repo) {
            Some(target) => Self::Remote(target),
            None => Self::Local {
                windows: cfg!(windows),
            },
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Local { windows: true } => "This Windows computer · OpenSSH".into(),
            Self::Local { windows: false } => "This computer · OpenSSH".into(),
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
            return Err("Choose an absolute key path on the indicated machine (~/ is also supported on SSH/WSL).".into());
        }
        if path.ends_with(['/', '\\']) || path.ends_with(".pub") {
            return Err("Choose the private-key filename, rather than its .pub file or a folder. Only its public companion is read for upload.".into());
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
                Err("This command must be constructed on the repository machine.".into())
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
                    return Err("Choose a valid SSH repository host.".into());
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
                    return Err("Choose a valid WSL distribution.".into());
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
                return Err("Remote setup scripts cannot run against a local key.".into());
            }
        };
        Ok(Invocation {
            shell,
            reserve_folder: None,
        })
    }
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
        return Err("Choose a new key folder beneath an existing folder. Existing folders are never reused.".into());
    }
    let separator = if matches!(machine, Machine::Local { windows: true }) {
        '\\'
    } else {
        '/'
    };
    let key = format!("{folder}{separator}id_ed25519");
    machine.validate_path(&key)?;
    text(title, "key title")?;
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
        return Err(
            "Choose an SSH destination such as git@github.com or git@forge.example:2222.".into(),
        );
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
    text(title, "key title")?;
    if machine.is_local() {
        return Err(
            "Local public-key registration must use the captured profile's account helper.".into(),
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
            return Err("Choose the existing gh username on the repository machine.".into());
        }
        // A named remote account overrides ambient tokens without changing global login.
        // The transient token travels only through a shell variable and command environment.
        format!(
            "set +x\nset -eu\nunset GH_DEBUG GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GH_HOST GH_REPO\nkey={}\nhost={}\nuser={}\ntoken=$(gh auth token --hostname \"$host\" --user \"$user\")\nexport GH_TOKEN=\"$token\" GH_ENTERPRISE_TOKEN=\"$token\"\nactual=$(gh api --hostname \"$host\" user --jq .login)\n[ \"$actual\" = \"$user\" ] || {{ printf '%s\\n' 'Account mismatch; public key was not uploaded.' >&2; exit 1; }}\ngh api --hostname \"$host\" --method POST user/keys --field {} --field \"key=@$key.pub\" >/dev/null\nprintf '%s\\n' 'Public key registered for the selected GitHub account.'\n",
            sh_path(key),
            sh_quote(host),
            sh_quote(user),
            sh_quote(&format!("title={title}"))
        )
    } else if let Some(login) = tea_login {
        text(login, "tea login alias")?;
        if login.is_empty() || login.starts_with('-') {
            return Err("Choose an existing tea login alias on the repository machine.".into());
        }
        // tea0.16+ exposes this explicit-login command. Its preview contains metadata
        // only; the user confirms that this alias's URL/account is the intended server.
        format!(
            "set +x\nset -eu\nkey={}\ntea login -- {}\nprintf '%s' 'Check the login URL/account above. Type UPLOAD to register this public key: '\nIFS= read -r answer\n[ \"$answer\" = UPLOAD ] || exit 1\nexec tea ssh-keys add --login {} --title {} -- \"$key.pub\"\n",
            sh_path(key),
            sh_quote(login),
            sh_quote(login),
            sh_quote(title)
        )
    } else {
        return Err("Choose GitHub or a Forgejo/Gitea tea login.".into());
    };
    machine.script(&script, "Register public SSH key there")
}

pub(super) fn text(value: &str, label: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) || value.len() > 512 {
        Err(format!(
            "The {label} cannot contain control characters and must fit within512 bytes."
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
            &invocation.shell.args[..3],
            &["-t", "--", "ssh://work@build:2222"]
        );
        let (_, payload) = invocation.shell.args[3]
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
}
