//! Remote listing protocol, parsing, and path navigation helpers.
use super::*;

/// Presentation from the original request values, never by splitting opaque paths or host names.
pub(super) fn display_list_error(raw: &str, dir: &str, host: &Host) -> String {
    let host = host.label();
    let args = [("dir", dir.into()), ("host", host.as_str().into())];
    if raw == format!("There is no folder {dir} on {host}.") {
        return crate::i18n::message("interaction-browser-missing", &args);
    }
    if raw == format!("Could not list {dir} on {host}.") {
        return crate::i18n::message("interaction-browser-list-invalid", &args);
    }
    if let Some(detail) = raw.strip_prefix(&format!("Could not list {dir} on {host}: ")) {
        return crate::i18n::message(
            "interaction-browser-list-failed",
            &[
                ("dir", dir.into()),
                ("host", host.as_str().into()),
                ("detail", detail.into()),
            ],
        );
    }
    let remote = crate::remote::display::diagnostic(raw);
    if remote == raw {
        crate::git::display_error(raw)
    } else {
        remote
    }
}

/// The script listing `dir`'s folders: the folder (as `cd` found it), whether it holds a
/// repository, then each folder, `R` or `D` before its name; NUL after each (a name can hold a
/// newline). `M` at the end: there were more.
pub(super) fn script(dir: &str) -> String {
    format!(
        concat!(
            "cd -- {dir} || exit 3\n",
            "printf '%s\\0' \"$(pwd)\"\n",
            "if [ -e .git ]; then printf 'R\\0'; else printf 'D\\0'; fi\n",
            "n=0\n",
            "for d in * .[!.]* ..?*; do\n",
            "  [ -d \"$d\" ] || continue\n",
            "  n=$((n+1))\n",
            "  if [ \"$n\" -gt {most} ]; then printf 'M\\0'; break; fi\n",
            "  if [ -e \"$d/.git\" ]; then printf 'R%s\\0' \"$d\"; else printf 'D%s\\0' \"$d\"; fi\n",
            "done\n",
        ),
        dir = remote::sh_path(dir),
        most = MOST,
    )
}

/// Lists `dir` on `host` (blocking: from a thread of its own).
pub(super) fn list(host: &Host, dir: &str) -> Result<Listing, String> {
    let out = remote::exec(
        host,
        &remote::Request {
            script: script(dir),
            ..remote::Request::default()
        },
    )?;
    if out.code == 3 {
        return Err(format!("There is no folder {dir} on {}.", host.label()));
    }
    if out.code != 0 {
        return Err(format!(
            "Could not list {dir} on {}: {}",
            host.label(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse(&out.stdout).ok_or_else(|| format!("Could not list {dir} on {}.", host.label()))
}

/// What [`script`] printed.
pub(super) fn parse(out: &[u8]) -> Option<Listing> {
    let mut parts = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    let dir = parts.next()?.into_owned();
    if !dir.starts_with('/') {
        return None;
    }
    let repo = parts.next()? == "R";
    let mut folders = Vec::new();
    let mut cut = false;
    for part in parts {
        let mut chars = part.chars();
        match chars.next() {
            Some('M') => cut = true,
            Some(kind @ ('R' | 'D')) => folders.push(Folder {
                name: chars.as_str().to_owned(),
                repo: kind == 'R',
            }),
            _ => {}
        }
    }
    folders.sort_by_key(|f| (f.name.starts_with('.'), f.name.to_lowercase()));
    Some(Listing {
        dir,
        repo,
        folders,
        cut,
    })
}

/// `name` in `dir`, both there.
pub(super) fn child(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The parts of `dir` to click on: (what is shown, the folder it opens).
pub(super) fn crumbs(dir: &str) -> Vec<(String, String)> {
    let mut list = vec![("/".to_owned(), "/".to_owned())];
    let mut path = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        path.push('/');
        path.push_str(part);
        list.push((part.to_owned(), path.clone()));
    }
    list
}
