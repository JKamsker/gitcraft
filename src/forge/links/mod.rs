//! Links to a repository's commits, branches, files and lines on its forge: built here for
//! every forge gitcraft knows, the remote to link to picked in [`remote`], and what a link
//! points at made a permalink (and checked to be pushed) in [`resolve`].

mod remote;
mod resolve;
#[cfg(test)]
mod tests;

pub use remote::{candidates, forge_name, linkable, upstream_remote};
pub use resolve::{Link, resolve};

use super::{Forge, Hosting};

/// Lines of a file, first to last (both included, counted from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lines {
    pub first: u32,
    pub last: u32,
}

impl Lines {
    /// The lines from `a` to `b`, whichever comes first; `None` when either is 0 (no line).
    pub fn new(a: u32, b: u32) -> Option<Lines> {
        (a > 0 && b > 0).then(|| Lines {
            first: a.min(b),
            last: a.max(b),
        })
    }

    /// "line 12" or "lines 10–14".
    pub fn label(self) -> String {
        if self.first == self.last {
            format!("line {}", self.first)
        } else {
            format!("lines {}–{}", self.first, self.last)
        }
    }
}

/// What a link is asked for: revisions as gitcraft has them (a hash, `HEAD`, `abc^`), which
/// [`resolve`] turns into the commit they name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Commit {
        rev: String,
    },
    /// A branch as the remote calls it (`main`, not `origin/main`).
    Branch {
        name: String,
    },
    /// A file (its path from the repository's root) at a commit, or some of its lines.
    File {
        rev: String,
        path: String,
        lines: Option<Lines>,
    },
}

/// What a link points at once its revision is a commit's full hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place<'a> {
    Commit(&'a str),
    Branch(&'a str),
    File {
        sha: &'a str,
        path: &'a str,
        lines: Option<Lines>,
    },
}

/// The page of `place` on the forge `hosting` is on. Each forge lays its pages out its own way:
///
/// | | commit | branch | file at a commit | lines |
/// |---|---|---|---|---|
/// | GitHub | `/commit/<sha>` | `/tree/<branch>` | `/blob/<sha>/<path>` | `?plain=1#L3-L5` |
/// | GitLab | `/-/commit/<sha>` | `/-/tree/<branch>` | `/-/blob/<sha>/<path>` | `?plain=1#L3-5` |
/// | Forgejo, Gitea | `/commit/<sha>` | `/src/branch/<branch>` | `/src/commit/<sha>/<path>` | `?display=source#L3-L5` |
///
/// The repository's page is `hosting.web` and its path, which keeps a server's port and the
/// folder it's installed under (`https://example.org/gitlab/group/repo`).
pub fn url(hosting: &Hosting, place: &Place) -> String {
    let repo = format!(
        "{}/{}",
        hosting.web.trim_end_matches('/'),
        encode_path(hosting.path.trim_matches('/'))
    );
    if hosting.forge == Forge::Bitbucket {
        return match place {
            Place::Commit(sha) => format!("{repo}/commits/{sha}"),
            Place::Branch(name) => format!("{repo}/src/{}", encode_segment(name)),
            Place::File { sha, path, lines } => {
                let mut out = format!("{repo}/src/{sha}/{}", encode_path(path));
                if let Some(l) = lines {
                    out.push_str(&format!("#lines-{}:{}", l.first, l.last));
                }
                out
            }
        };
    }
    if hosting.forge == Forge::Azure {
        return match place {
            Place::Commit(sha) => format!("{repo}/commit/{sha}"),
            Place::Branch(name) => format!("{repo}?version=GB{}", encode_segment(name)),
            Place::File { sha, path, lines } => {
                let mut url = format!(
                    "{repo}?path={}&version=GC{sha}&_a=contents",
                    encode_segment(&format!("/{path}"))
                );
                if let Some(l) = lines {
                    url.push_str(&format!(
                        "&line={}&lineEnd={}&lineStartColumn=1&lineEndColumn=1",
                        l.first,
                        l.last.saturating_add(1)
                    ));
                }
                url
            }
        };
    }
    let gitlab = hosting.forge == Forge::GitLab;
    let dash = if gitlab { "/-" } else { "" };
    match place {
        Place::Commit(sha) => format!("{repo}{dash}/commit/{sha}"),
        Place::Branch(name) => {
            let tree = match hosting.forge {
                Forge::GitHub | Forge::GitLab | Forge::Azure | Forge::Bitbucket => "tree",
                Forge::Forgejo | Forge::Gitea => "src/branch",
            };
            format!("{repo}{dash}/{tree}/{}", encode_path(name))
        }
        Place::File { sha, path, lines } => {
            let blob = match hosting.forge {
                Forge::GitHub | Forge::GitLab | Forge::Azure | Forge::Bitbucket => "blob",
                Forge::Forgejo | Forge::Gitea => "src/commit",
            };
            let mut out = format!("{repo}{dash}/{blob}/{sha}/{}", encode_path(path));
            if let Some(lines) = lines {
                // A Markdown file (or a notebook, CSV, SVG…) shows rendered, where a line's
                // anchor goes nowhere: ask for its source.
                out.push_str(match hosting.forge {
                    Forge::GitHub | Forge::GitLab | Forge::Azure | Forge::Bitbucket => "?plain=1",
                    Forge::Forgejo | Forge::Gitea => "?display=source",
                });
                out.push_str(&anchor(hosting.forge, *lines));
            }
            out
        }
    }
}

/// `#L3`, `#L3-L5` (GitLab: `#L3-5`).
fn anchor(forge: Forge, lines: Lines) -> String {
    if lines.first == lines.last {
        return format!("#L{}", lines.first);
    }
    let end = if forge == Forge::GitLab { "" } else { "L" };
    format!("#L{}-{end}{}", lines.first, lines.last)
}

/// `path` for a URL: each of its parts percent-encoded (spaces, `#`, `?`, `%`, anything not
/// ASCII), the `/` between them kept.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
            );
        if keep {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
