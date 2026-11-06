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
        // Docker Desktop's own, which has no git and no repositories.
        .filter(|l| !l.starts_with("docker-desktop"))
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
            // `Include config.d/*`: every file it matches, in order, as ssh reads them.
            for file in expand(&path) {
                read_config(&file, ssh_dir, hosts, depth + 1);
            }
        }
    }
}

/// The files `pattern` names: itself, or with `*` and `?` in its parts, the files matching it
/// (as `glob` matches them: not a hidden file for a `*`), sorted.
fn expand(pattern: &Path) -> Vec<PathBuf> {
    let mut found = vec![PathBuf::new()];
    for part in pattern.components() {
        let name = part.as_os_str().to_string_lossy();
        if !name.contains(['*', '?']) {
            for path in &mut found {
                path.push(part);
            }
            continue;
        }
        let mut next = Vec::new();
        for dir in &found {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut matched: Vec<PathBuf> = entries
                .flatten()
                .filter(|e| wildcard_match(&name, &e.file_name().to_string_lossy()))
                .map(|e| e.path())
                .collect();
            matched.sort();
            next.extend(matched);
        }
        found = next;
    }
    found
}

/// Whether `name` matches `pattern`, of `*` (any run) and `?` (any one); a leading `.` only
/// matched by one in the pattern.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    // Where the last `*` was, and where in the name it was tried up to.
    let (mut pi, mut ni, mut star) = (0, 0, None);
    while ni < n.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ni));
                pi += 1;
            }
            Some(&c) if c == '?' || c == n[ni] => {
                pi += 1;
                ni += 1;
            }
            _ => match star {
                Some((sp, sn)) => {
                    pi = sp + 1;
                    ni = sn + 1;
                    star = Some((sp, sn + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The words of a config line's value, as ssh splits them: at blanks, but not inside double or
/// single quotes (which are taken off).
fn config_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut in_word = false;
    for c in value.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// The plain host names of `Host` lines (without patterns, which name no host) and the files
/// of `Include` lines (patterns kept: [`expand`] finds their files).
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
        let words = config_words(rest.trim_start_matches(|c: char| c.is_whitespace() || c == '='));
        if key.eq_ignore_ascii_case("host") {
            hosts.extend(
                words
                    .into_iter()
                    .filter(|w| !w.contains(['*', '?', '!']) && super::valid_ssh_host(w)),
            );
        } else if key.eq_ignore_ascii_case("include") {
            includes.extend(words);
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
        assert_eq!(parse_distros(&bytes), ["Ubuntu"]);
    }

    #[test]
    fn config_hosts_leave_out_patterns() {
        let (hosts, includes) = parse_config(
            "Host build  box-2\n  HostName 10.0.0.1\n# Host hidden\nHost *.corp !x\nhost=dev\nInclude conf.d/*  extra.conf\n",
        );
        assert_eq!(hosts, ["build", "box-2", "dev"]);
        assert_eq!(includes, ["conf.d/*", "extra.conf"]);
    }

    /// Quoted values are one word, without their quotes.
    #[test]
    fn quoted_values_are_one_word() {
        let (hosts, includes) =
            parse_config("Host \"quoted\" 'single'\nInclude \"My Configs/work.conf\" other\n");
        assert_eq!(hosts, ["quoted", "single"]);
        assert_eq!(includes, ["My Configs/work.conf", "other"]);
        assert_eq!(config_words("  a  \"b c\"d  "), ["a", "b cd"]);
    }

    #[test]
    fn wildcards_match_as_glob_does() {
        assert!(wildcard_match("*", "work.conf"));
        assert!(wildcard_match("*.conf", "work.conf"));
        assert!(wildcard_match("w?rk*", "work.conf"));
        assert!(!wildcard_match("*.conf", "work.conf.bak"));
        assert!(!wildcard_match("*", ".hidden"));
        assert!(wildcard_match(".*", ".hidden"));
        assert!(wildcard_match("a*b*c", "aXbYbZc"));
        assert!(!wildcard_match("a*b*c", "aXbYbZ"));
    }

    /// `Include config.d/*` reads every file there, in order, and the hosts in them are offered.
    #[test]
    fn wildcard_includes_are_read() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-ssh-{}", std::process::id()));
        let conf_d = dir.join("config.d");
        std::fs::create_dir_all(&conf_d).map_err(|e| e.to_string())?;
        let write = |path: PathBuf, text: &str| std::fs::write(path, text);
        let made = write(dir.join("config"), "Include config.d/*\nHost main\n")
            .and_then(|()| write(conf_d.join("b.conf"), "Host second\n"))
            .and_then(|()| write(conf_d.join("a.conf"), "Host first\n"))
            .and_then(|()| write(conf_d.join(".hidden"), "Host hidden\n"));
        let mut hosts = Vec::new();
        if made.is_ok() {
            read_config(&dir.join("config"), &dir, &mut hosts, 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
        made.map_err(|e| e.to_string())?;
        assert_eq!(hosts, ["main", "first", "second"]);
        Ok(())
    }
}
