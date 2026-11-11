//! Links into gitcraft: `gitcraft://open?url=<a forge's web URL>` from a browser, or that web
//! URL pasted into the palette or the start page. What the URL points at is read from its path
//! alone, as GitHub, GitLab, Gitea / Forgejo (Codeberg) and Bitbucket lay theirs out:
//!
//! ```text
//! https://github.com/o/r[/pull/12 | /commit/<sha> | /blob/<rev>/<path>#L40]
//! https://gitlab.com/g/sub/r[/-/merge_requests/12 | /-/commit/<sha> | /-/blob/<rev>/<path>]
//! https://codeberg.org/o/r[/pulls/12 | /commit/<sha> | /src/branch/<b>/<path>#L40]
//! https://bitbucket.org/o/r[/pull-requests/12 | /commits/<sha> | /src/<rev>/<path>#lines-40]
//! ```
//!
//! Everything that comes in is checked strictly: anything else is refused, with why.

/// The longest link taken.
const MAX_LINK: usize = 4096;

/// A forge's page, as far as gitcraft can open it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLink {
    /// The link came over plain `http://` (a forge serving only that is cloned so too).
    pub http: bool,
    /// The web host, lowercase, with its port if one was given (`git.example.com:3000`).
    pub host: String,
    /// The repository's path on the forge (`owner/repo`, `group/sub/repo`).
    pub repo: String,
    pub target: Target,
    /// The link as given, to show.
    pub url: String,
}

/// What on the repository's pages the link points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Repo,
    PullRequest(u64),
    Commit(String),
    /// A file at a revision, maybe at a line.
    File {
        rev: String,
        path: String,
        line: Option<u32>,
    },
}

/// A `gitcraft://` link: `gitcraft://open?url=<percent-encoded web URL>`.
pub fn parse_scheme_link(link: &str) -> Result<ForgeLink, String> {
    let bad = |why: &str| format!("Not a gitcraft link gitcraft can open ({why})");
    if link.len() > MAX_LINK {
        return Err(bad("too long"));
    }
    let rest =
        strip_prefix_ignore_case(link, "gitcraft://").ok_or_else(|| bad("no gitcraft://"))?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (action, query) = rest.split_once('?').ok_or_else(|| bad("no ?url="))?;
    let action = action.strip_suffix('/').unwrap_or(action);
    if !action.eq_ignore_ascii_case("open") {
        return Err(bad("it only opens: gitcraft://open?url=…"));
    }
    let mut url = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("url", value)) if url.is_none() => url = Some(percent_decode(value)?),
            Some(("url", _)) => return Err(bad("url is given twice")),
            _ => return Err(bad("only url= is taken")),
        }
    }
    parse_web_url(&url.ok_or_else(|| bad("no url="))?)
}

/// A `gitcraft://` link or a forge's web URL.
pub fn parse_any(link: &str) -> Result<ForgeLink, String> {
    if link
        .get(..9)
        .is_some_and(|h| h.eq_ignore_ascii_case("gitcraft:"))
    {
        parse_scheme_link(link)
    } else {
        parse_web_url(link)
    }
}

/// A forge's web URL, as a browser shows it.
pub fn parse_web_url(url: &str) -> Result<ForgeLink, String> {
    let url = url.trim();
    let bad = |why: &str| format!("{} is not a link gitcraft can open: {why}", shown(url));
    if url.len() > MAX_LINK {
        return Err(bad("it is too long"));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(bad("it has spaces or control characters"));
    }
    let rest = strip_prefix_ignore_case(url, "https://")
        .or_else(|| strip_prefix_ignore_case(url, "http://"))
        .ok_or_else(|| bad("only https:// (or http://) links are opened"))?;
    let (rest, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let rest = rest.split_once('?').map_or(rest, |(r, _)| r);
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = valid_host(authority).ok_or_else(|| bad("its host isn't a host name"))?;
    let segments = path_segments(path).ok_or_else(|| bad("its path isn't a forge's"))?;
    let (repo, target) = read_path(&segments, fragment)
        .ok_or_else(|| bad("it doesn't point at a repository, pull request, commit or file"))?;
    let http = strip_prefix_ignore_case(url, "http://").is_some();
    Ok(ForgeLink {
        http,
        host,
        repo,
        target,
        url: url.to_owned(),
    })
}

/// A host name (and port) as forges have them, lowercase: letters, digits, `-` and `.`, no
/// user part (`user@`), no IP address tricks beyond plain digits and dots.
fn valid_host(authority: &str) -> Option<String> {
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    let fine_host = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-'))
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    let fine_port = port.is_none_or(|p| {
        p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok_and(|n| n > 0)
    });
    (fine_host && fine_port).then(|| authority.to_ascii_lowercase())
}

/// The path's names, decoded; `None` when one is empty after the first, `.` or `..`, or
/// holds a `/`, `\` or control character once decoded.
fn path_segments(path: &str) -> Option<Vec<String>> {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return Some(Vec::new());
    }
    path.split('/')
        .map(|raw| {
            let name = percent_decode(raw).ok()?;
            let fine = !name.is_empty()
                && name != "."
                && name != ".."
                && !name.contains(['/', '\\'])
                && !name.chars().any(char::is_control);
            fine.then_some(name)
        })
        .collect()
}

/// The repository and what in it, from the path's names.
fn read_path(s: &[String], fragment: &str) -> Option<(String, Target)> {
    let line = line_of(fragment);
    // GitLab: `group/…/repo/-/what…`, told by what follows the `-` (a folder called `-` in
    // another forge's path is followed by anything else).
    const GITLAB: [&str; 4] = ["merge_requests", "commit", "blob", "tree"];
    let dash = s.iter().enumerate().position(|(i, n)| {
        n == "-" && i >= 2 && s.get(i + 1).is_none_or(|k| GITLAB.contains(&k.as_str()))
    });
    if let Some(dash) = dash {
        let repo = s.get(..dash)?.join("/");
        let what = s.get(dash + 1..)?;
        let target = match what {
            [] => Target::Repo,
            [mr, n, ..] if mr == "merge_requests" => Target::PullRequest(number(n)?),
            [c, sha] if c == "commit" => Target::Commit(hash(sha)?),
            [b, rev, path @ ..] if b == "blob" && !path.is_empty() => file(rev, path, line)?,
            // A folder: the repository.
            [t, ..] if t == "tree" => Target::Repo,
            _ => return None,
        };
        return Some((repo, target));
    }
    let (owner, name) = (s.first()?, s.get(1)?);
    let repo = format!("{owner}/{}", name.strip_suffix(".git").unwrap_or(name));
    let target = match &s[2..] {
        [] => Target::Repo,
        // GitHub `pull`, Gitea / Forgejo `pulls`, Bitbucket `pull-requests`, with a tab after.
        [p, n, ..] if p == "pull" || p == "pulls" || p == "pull-requests" => {
            Target::PullRequest(number(n)?)
        }
        // `commit/<sha>`, Bitbucket `commits/<sha>`.
        [c, sha, ..] if c == "commit" || c == "commits" => Target::Commit(hash(sha)?),
        // GitHub `blob/<rev>/<path>`.
        [b, rev, path @ ..] if b == "blob" && !path.is_empty() => file(rev, path, line)?,
        // Gitea / Forgejo `src/branch|tag|commit/<rev>/<path>`.
        [src, kind, rev, path @ ..]
            if src == "src"
                && matches!(kind.as_str(), "branch" | "tag" | "commit")
                && !path.is_empty() =>
        {
            file(rev, path, line)?
        }
        // A Gitea / Forgejo folder or branch page (`src/branch/main`): the repository.
        [src, kind, ..] if src == "src" && matches!(kind.as_str(), "branch" | "tag" | "commit") => {
            Target::Repo
        }
        // Bitbucket `src/<rev>/<path>`.
        [src, rev, path @ ..] if src == "src" && !path.is_empty() => file(rev, path, line)?,
        [t, ..] if t == "tree" || t == "src" => Target::Repo,
        _ => return None,
    };
    Some((repo, target))
}

fn file(rev: &str, path: &[String], line: Option<u32>) -> Option<Target> {
    let rev = crate::launch::args::rev_text(&rev.into()).ok()?;
    Some(Target::File {
        rev,
        path: path.join("/"),
        line,
    })
}

/// A pull request's number.
fn number(s: &str) -> Option<u64> {
    (s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
        .filter(|&n| n > 0)
}

/// A commit's hash, abbreviated or whole.
fn hash(s: &str) -> Option<String> {
    (4..=64)
        .contains(&s.len())
        .then_some(())
        .filter(|()| s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|()| s.to_ascii_lowercase())
}

/// The line a fragment points at: `L40`, `L40-L45`, `L40C3`, Bitbucket's `lines-40`.
fn line_of(fragment: &str) -> Option<u32> {
    let digits = fragment
        .strip_prefix('L')
        .or_else(|| fragment.strip_prefix("lines-"))?;
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok().filter(|&n| n > 0)
}

/// `%xx` decoded, as UTF-8; `+` stays `+` (it is a path, and the query's URL is encoded whole).
fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or("a broken %-escape")?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "it isn't valid text".to_owned())
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// A link in a message: at most a line's worth.
fn shown(url: &str) -> String {
    let short: String = url.chars().take(120).collect();
    let short = short.escape_debug().to_string();
    if url.chars().count() > 120 {
        format!("{short}…")
    } else {
        short
    }
}

impl ForgeLink {
    /// Whether the remote `url` (`git@host:o/r.git`, `https://host/o/r`…) is this repository.
    pub fn is_remote(&self, url: &str) -> bool {
        let Some((host, path)) = crate::forge::location(url) else {
            return false;
        };
        let link_host = self.host.split(':').next().unwrap_or(&self.host);
        host.eq_ignore_ascii_case(link_host) && path.eq_ignore_ascii_case(&self.repo)
    }

    /// The URL to clone it from, over HTTPS.
    pub fn clone_url(&self) -> String {
        let scheme = if self.http { "http" } else { "https" };
        format!("{scheme}://{}/{}.git", self.host, self.repo)
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
