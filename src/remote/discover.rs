//! What the open-remote dialog offers: the WSL distributions installed, the hosts of the
//! user's SSH config.

use std::path::{Path, PathBuf};

/// The WSL distributions installed (`wsl.exe -l -q`), none where there is no WSL.
pub fn wsl_distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.args(["-l", "-q"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(out) if out.status.success() => parse_distros(&out.stdout),
        _ => Vec::new(),
    }
}

/// `wsl.exe -l` speaks UTF-16 (little-endian), one name a line.
fn parse_distros(bytes: &[u8]) -> Vec<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| super::valid_distro(l))
        .map(str::to_owned)
        .collect()
}

/// The hosts named in `~/.ssh/config` (and the files it includes), without patterns.
pub fn ssh_config_hosts() -> Vec<String> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let dir = home.join(".ssh");
    let mut hosts = Vec::new();
    read_config(&dir.join("config"), &dir, &mut hosts, 0);
    hosts
}

fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|v| std::env::var_os(v).filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

fn read_config(file: &Path, ssh_dir: &Path, hosts: &mut Vec<String>, depth: u32) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let (found, includes) = parse_config(&text);
    for host in found {
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    // Includes of includes, but not forever.
    if depth < 4 {
        for inc in includes {
            let path = Path::new(&inc);
            let path = if path.is_absolute() {
                path.to_owned()
            } else if let Some(rest) = inc.strip_prefix("~/") {
                ssh_dir.parent().unwrap_or(ssh_dir).join(rest)
            } else {
                ssh_dir.join(path)
            };
            read_config(&path, ssh_dir, hosts, depth + 1);
        }
    }
}

/// The plain host names of `Host` lines and the files of `Include` lines (without patterns,
/// which name no host and no one file).
fn parse_config(text: &str) -> (Vec<String>, Vec<String>) {
    let mut hosts = Vec::new();
    let mut includes = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let (key, rest) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or((line, ""));
        let words = rest
            .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
            .split_whitespace()
            .filter(|w| !w.contains(['*', '?', '!']));
        if key.eq_ignore_ascii_case("host") {
            hosts.extend(
                words
                    .filter(|w| super::valid_ssh_host(w))
                    .map(str::to_owned),
            );
        } else if key.eq_ignore_ascii_case("include") {
            includes.extend(words.map(str::to_owned));
        }
    }
    (hosts, includes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distros_come_from_utf16() {
        let text = "Ubuntu\r\ndocker-desktop\r\n\r\n";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(parse_distros(&bytes), ["Ubuntu", "docker-desktop"]);
    }

    #[test]
    fn config_hosts_leave_out_patterns() {
        let (hosts, includes) = parse_config(
            "Host build  box-2\n  HostName 10.0.0.1\n# Host hidden\nHost *.corp !x\nhost=dev\nInclude conf.d/*  extra.conf\n",
        );
        assert_eq!(hosts, ["build", "box-2", "dev"]);
        assert_eq!(includes, ["extra.conf"]);
    }
}
