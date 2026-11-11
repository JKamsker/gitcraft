//! System identification and personal-path redaction for diagnostic reports.

/// The system: its name and version where it says them, and the processor.
pub(super) fn system() -> String {
    let arch = std::env::consts::ARCH;
    let name = os_name().unwrap_or_else(|| std::env::consts::OS.to_owned());
    format!("{name} ({arch})")
}

#[cfg(target_os = "linux")]
pub(super) fn os_name() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    pretty_name(&text)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn os_name() -> Option<String> {
    None
}

/// `PRETTY_NAME` of an os-release file, unquoted.
#[cfg(any(target_os = "linux", test))]
pub fn pretty_name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim().trim_matches('"').to_owned())
        .filter(|v| !v.is_empty())
}

/// This user's home folder, as the report writes it `~`.
pub(super) fn home() -> String {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `text` with the home folder (as written with `\` or `/`, in any case) replaced by `~`, where
/// it is a whole path (followed by a separator, a space or the end).
pub fn redact_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 2 {
        return text.to_owned();
    }
    let mut out = text.to_owned();
    for form in [home.to_owned(), home.replace('\\', "/")] {
        out = replace_whole(&out, &form, "~");
    }
    out
}

/// `text` with every `needle` (ASCII case ignored) that ends where a path component does
/// replaced by `with`.
pub(super) fn replace_whole(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let wanted = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&wanted) {
        let start = at + found;
        let end = start + wanted.len();
        let whole = text[end..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '/' | '\\' | '`') || c.is_whitespace());
        out.push_str(&text[at..start]);
        out.push_str(if whole { with } else { &text[start..end] });
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with the folder named after any user written `~`: what follows `/home/`, `/Users/`
/// or `\Users\` (also `/mnt/c/Users/`, `/c/Users/`, `C:/Users/`), and `/root`, on whichever
/// machine (WSL, SSH, Windows) the path is.
pub fn redact_users(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let next = ["/home/", "/users/", "\\users\\"]
            .iter()
            .filter_map(|m| lower[at..].find(m).map(|i| (at + i, m.len())))
            .min();
        let Some((start, len)) = next else {
            break;
        };
        // Back to the start of the path: a drive (`C:`), `/mnt/c`, `/c`.
        let path_start = text[at..start]
            .rfind(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | '='))
            .map_or(at, |i| at + i + 1);
        let name_end = text[start + len..]
            .find(|c: char| matches!(c, '/' | '\\' | '`') || c.is_whitespace())
            .map_or(text.len(), |i| start + len + i);
        out.push_str(&text[at..path_start]);
        out.push('~');
        at = name_end;
    }
    out.push_str(&text[at..]);
    replace_whole(&out, "/root", "~")
}
