//! Profile-bound SSH identity validation and helper execution.
use super::*;

/// SSH command-line identities are additive with ~/.ssh/config's IdentityFile entries.
/// Refuse another effective identity rather than silently pushing as another account.
pub(crate) fn validate_ssh_identities(
    key: &str,
    config: &str,
    ssh_args: &[String],
) -> Result<(), String> {
    if config.lines().any(|line| {
        line.strip_prefix("certificatefile ")
            .is_some_and(|value| value != "none")
    }) {
        return Err("This profile selects a key file, not a certificate identity. Remove other CertificateFile settings for this host before using it.".into());
    }
    let mut seen = false;
    let normalized = |value: &str| {
        let path = PathBuf::from(value);
        let path = path.canonicalize().unwrap_or(path);
        #[cfg(windows)]
        let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
        #[cfg(not(windows))]
        let text = path.to_string_lossy().into_owned();
        text
    };
    let selected = normalized(key);
    for identity in config
        .split_terminator('\n')
        .filter_map(|line| line.strip_prefix("identityfile "))
    {
        if identity.chars().any(char::is_control) {
            return Err(
                "SSH returned an ambiguous identity path containing control characters".into(),
            );
        }
        if identity == "none" {
            continue;
        }
        let expanded;
        let identity =
            if identity.starts_with('~') || identity.contains('%') || identity.contains("${") {
                // `ssh -G` leaves IdentityFile unexpanded. ControlPath uses OpenSSH's same
                // tilde/environment/percent expansion, including passwd home and %C, and -G
                // prints that expanded value without opening a connection or reading a key.
                // Expansion is a -G-only probe: this control path is data, never a
                // transport. All actual connections use ControlPath=none.
                // Fence the complete raw expansion: a Unix filename can contain LF/CR.
                // Taking only its first line would accept a different key's selected prefix.
                let fence = "__GITCRAFT_IDENTITY_END__";
                let tagged = format!("{identity}{fence}");
                let quoted = tagged.replace('\\', "\\\\").replace('"', "\\\"");
                let mut cmd = ssh_command_with_control_path(key, &format!("\"{quoted}\""));
                cmd.arg("-G").args(ssh_args);
                let output = read_private(cmd)?;
                expanded = expanded_identity(&output, fence)?;
                expanded.as_str()
            } else {
                identity
            };
        if normalized(identity) != selected {
            return Err("SSH configuration offers another IdentityFile for this host. Keep only the profile's selected identity for this host before pushing.".to_owned());
        }
        seen = true;
    }
    if !seen {
        return Err("SSH could not select this profile's identity file".to_owned());
    }
    Ok(())
}

/// Read one fenced value without line splitting, CR removal, or whitespace trimming.
/// `rfind` includes any embedded fence in the path; a later field containing a fence
/// makes the result fail closed because the intervening newlines remain in the value.
pub(super) fn expanded_identity(output: &str, fence: &str) -> Result<String, String> {
    let mut fields = output
        .match_indices("controlpath ")
        .filter(|(at, _)| *at == 0 || output.as_bytes().get(at - 1) == Some(&b'\n'));
    let start = fields
        .next()
        .ok_or("SSH did not expand the configured identity path")?
        .0
        + "controlpath ".len();
    if fields.next().is_some() {
        return Err("SSH returned ambiguous identity expansion fields".into());
    }
    let end = output
        .rfind(fence)
        .filter(|end| *end >= start)
        .ok_or("SSH did not finish expanding the configured identity path")?;
    let tail = &output[end + fence.len()..];
    if !tail.starts_with('\n') && !tail.starts_with("\r\n") {
        return Err("SSH returned an ambiguous identity expansion boundary".into());
    }
    let value = &output[start..end];
    if value.chars().any(char::is_control) {
        return Err("SSH returned an identity path containing control characters".into());
    }
    Ok(value.to_owned())
}

pub(crate) fn ssh_helper_command(key: &str, executable: &Path) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(key.as_bytes());
    #[cfg(windows)]
    let executable = PathBuf::from(executable.to_string_lossy().replace('\\', "/"));
    format!(
        "{} --gitcraft-profile-ssh {encoded}",
        shell_quote(&executable.to_string_lossy())
    )
}

pub(super) fn ssh_command(key: &str) -> Command {
    ssh_command_with_control_path(key, "none")
}

pub(super) fn ssh_command_with_control_path(key: &str, control_path: &str) -> Command {
    let mut cmd = Command::new("ssh");
    crate::shell_path::apply(&mut cmd);
    // First-value policy is fixed before user config/operands. No other account's
    // authenticated master, agent, certificate or password can replace this key.
    cmd.args([
        "-o",
        &format!("ControlPath={control_path}"),
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPersist=no",
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        "IdentityAgent=none",
        "-o",
        "CertificateFile=none",
        "-o",
        "PKCS11Provider=none",
        "-o",
        "ForwardAgent=no",
        "-o",
        "PreferredAuthentications=publickey",
        "-i",
        key,
    ]);
    cmd
}

pub fn ssh_helper() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--gitcraft-profile-ssh") {
        return None;
    }
    let run = || -> Result<i32, String> {
        let [_, encoded, rest @ ..] = args.as_slice() else {
            return Err("Invalid profile SSH request".to_owned());
        };
        let key = String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| "Invalid profile SSH key path")?,
        )
        .map_err(|_| "Invalid profile SSH key path")?;
        run_ssh(&key, rest)
    };
    Some(match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gitcraft profile SSH: {error}");
            1
        }
    })
}

/// Git's Plink dialect is not OpenSSH: notably -P is a port in Plink and a tag in
/// modern OpenSSH. Validate before even the -G probe, so an explicit local variant
/// cannot silently change the endpoint. Operand values and the remote command are data.
pub(super) fn validate_ssh_options(args: &[String]) -> Result<(), String> {
    const BAD: &str = "This profile's key uses OpenSSH, but Git supplied incompatible SSH options. Set this repository's ssh.variant to auto or ssh, or use its own matching SSH command.";
    let mut at = 0;
    let mut have_host = false;
    while let Some(arg) = args.get(at) {
        if arg == "--" {
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            // OpenSSH parses options again after its destination, until the remote
            // command (or --) starts. Plink options there are just as ambiguous.
            if have_host {
                break;
            }
            have_host = true;
            at += 1;
            continue;
        }
        if arg == "-batch" {
            return Err(BAD.into());
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(option) = chars.next() {
            if "PMSOAIi".contains(option) {
                return Err(BAD.into());
            }
            if "1246AaCfGgKkMNnqsTtVvXxYy".contains(option) {
                continue;
            }
            if "BbcDEeFIiJLlmOopQRSWw".contains(option) {
                if chars.peek().is_none() {
                    at += 1;
                    if args.get(at).is_none() {
                        return Err(BAD.into());
                    }
                }
                break;
            }
            return Err(BAD.into());
        }
        at += 1;
    }
    Ok(())
}

/// Shared with the helper-boundary fixtures: these are the exact checks and transport
/// launch used by --gitcraft-profile-ssh, without reading any key contents in gitcraft.
pub(super) fn run_ssh(key: &str, rest: &[String]) -> Result<i32, String> {
    if !Path::new(key).is_absolute() || key.chars().any(char::is_control) {
        return Err("Invalid profile SSH key path".to_owned());
    }
    validate_ssh_options(rest)?;
    let mut inspect = ssh_command(key);
    inspect.arg("-G").args(rest);
    let config = read_private(inspect)?;
    validate_ssh_identities(key, &config, rest)?;
    let status = ssh_command(key)
        .args(rest)
        .status()
        .map_err(|_| "Could not start SSH")?;
    Ok(status.code().unwrap_or(1))
}
