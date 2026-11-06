//! The path patterns of GitHub's and GitLab's CODEOWNERS: gitignore's, as those forges read them.
//!
//! - A pattern with a `/` at its start or in its middle is from the repository's root; one
//!   without (`*.js`, `apps/`) matches a file or folder of that name at any depth.
//! - A pattern that matches a folder matches every file under it (`/docs/`, `apps`), except one
//!   ending in `/*`, which matches only the files right in that folder.
//! - A trailing `/` matches only folders.
//! - `*` and `?` stop at `/`; `**` crosses folders (`**/logs`, `a/**/b`, `a/**`); `[a-z]` is a
//!   class; `\` takes the next character as it is (`\#`, `\ `).

/// A pattern, ready to match paths with: a regular expression made from it once.
#[derive(Clone, Debug)]
pub(crate) struct Pattern {
    regex: Option<regex::Regex>,
    #[cfg(test)]
    wild: bool,
}

impl Pattern {
    pub(crate) fn new(pattern: &str) -> Self {
        let dir_only = pattern.ends_with('/') && !pattern.ends_with("\\/");
        let trimmed = pattern.trim_end_matches('/');
        let anchored = trimmed.starts_with('/') || trimmed.trim_start_matches('/').contains('/');
        let glob = trimmed.trim_start_matches('/');
        // `docs/*`: only the files right in the folder. The pattern alone, as `*`, stays
        // everything.
        let shallow = glob.ends_with("/*") && !glob.ends_with("*/*");
        let body = translate(glob);
        let lead = if anchored { "^" } else { "^(?:.*/)?" };
        // What may follow what the pattern matched: the files under a folder it matched.
        let tail = match (shallow, dir_only) {
            (true, _) => "",
            (false, true) => "/.+",
            (false, false) => "(?:/.+)?",
        };
        Pattern {
            regex: (!glob.is_empty())
                .then(|| regex::Regex::new(&format!("{lead}{body}{tail}$")).ok())
                .flatten(),
            #[cfg(test)]
            wild: glob.contains(['*', '?', '[']),
        }
    }

    /// Whether the file at `path` (from the root, `/`-separated) is matched.
    pub(crate) fn matches(&self, path: &str) -> bool {
        let path = path.trim_matches('/');
        !path.is_empty() && self.regex.as_ref().is_some_and(|r| r.is_match(path))
    }

    /// Whether `**` or `*` make it match more than one name.
    #[cfg(test)]
    fn is_wild(&self) -> bool {
        self.wild
    }
}

/// The regular expression of a glob: `*` and `?` within a name, `**` across names, `[…]` a
/// class, `\x` the character x.
fn translate(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                let at_start = i == 0 || chars[i - 1] == '/';
                if at_start && chars.get(i + 2) == Some(&'/') {
                    // `**/`: any folders, or none.
                    out.push_str("(?:.*/)?");
                    i += 3;
                } else {
                    out.push_str(".*");
                    i += 2;
                }
            }
            '*' => {
                out.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            '[' => match class_end(&chars, i) {
                Some(end) => {
                    out.push('[');
                    let mut j = i + 1;
                    if matches!(chars.get(j), Some('!' | '^')) {
                        out.push('^');
                        j += 1;
                    }
                    for &c in &chars[j..end] {
                        if matches!(c, '\\' | '[' | ']' | '&' | '~' | '^') {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                    out.push(']');
                    i = end + 1;
                }
                None => {
                    out.push_str("\\[");
                    i += 1;
                }
            },
            '\\' => {
                let literal = chars.get(i + 1).copied().unwrap_or('\\');
                out.push_str(&regex::escape(&literal.to_string()));
                i += 2;
            }
            c => {
                out.push_str(&regex::escape(&c.to_string()));
                i += 1;
            }
        }
    }
    out
}

/// Where the class opened at `chars[start]` closes, if it does (a `]` right after the `[`, or
/// its `!`, is in the class).
fn class_end(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start + 1;
    if matches!(chars.get(j), Some('!' | '^')) {
        j += 1;
    }
    if chars.get(j) == Some(&']') {
        j += 1;
    }
    (j..chars.len()).find(|&k| chars[k] == ']')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        Pattern::new(pattern).matches(path)
    }

    /// The examples of GitHub's documentation of CODEOWNERS.
    #[test]
    fn githubs_examples() {
        assert!(m("*", "a/b/c.txt"));
        assert!(m("*.js", "src/app/main.js"));
        assert!(!m("*.js", "src/app/main.jsx"));
        assert!(m("*.go", "main.go"));
        assert!(m("/build/logs/", "build/logs/today/x.log"));
        assert!(!m("/build/logs/", "src/build/logs/x.log"));
        assert!(m("docs/*", "docs/getting-started.md"));
        assert!(!m("docs/*", "docs/build-app/troubleshooting.md"));
        assert!(m("apps/", "apps/x.txt"));
        assert!(m("apps/", "src/apps/deep/x.txt"));
        assert!(!m("apps/", "apps"), "a file named apps is not a folder");
        assert!(m("/docs/", "docs/a/b.md"));
        assert!(!m("/docs/", "x/docs/a.md"));
        assert!(m("/scripts/", "scripts/run.sh"));
        assert!(m("**/logs", "build/logs/a.log"));
        assert!(m("**/logs", "logs/a.log"));
        assert!(m("/apps/github", "apps/github/x.rb"));
        assert!(m("/apps/github", "apps/github"));
        assert!(!m("/apps/github", "apps/githubx/x.rb"));
    }

    #[test]
    fn a_name_matches_a_file_or_a_folder_at_any_depth() {
        assert!(m("Makefile", "Makefile"));
        assert!(m("Makefile", "sub/Makefile"));
        assert!(m("vendor", "a/vendor/lib/x.c"));
        assert!(!m("vendor", "vendored/x.c"));
    }

    #[test]
    fn stars_stay_within_a_name_and_double_stars_cross_folders() {
        assert!(m("/src/*.rs", "src/main.rs"));
        assert!(!m("/src/*.rs", "src/a/main.rs"));
        assert!(m("/src/**/*.rs", "src/main.rs"));
        assert!(m("/src/**/*.rs", "src/a/b/main.rs"));
        assert!(m("/src/**", "src/a/b/main.rs"));
        assert!(m("a/**/b", "a/x/y/b"));
        assert!(m("a/**/b", "a/b"));
        assert!(m("/f?o.txt", "foo.txt"));
        assert!(!m("/f?o.txt", "f/o.txt"));
    }

    #[test]
    fn classes_and_escapes() {
        assert!(m("/[a-c].txt", "b.txt"));
        assert!(!m("/[a-c].txt", "d.txt"));
        assert!(m("/[!a-c].txt", "d.txt"));
        assert!(m("/\\#notes.md", "#notes.md"));
        assert!(m("/my\\ file.txt", "my file.txt"));
        assert!(m("/[unclosed", "[unclosed"));
    }

    #[test]
    fn unicode_and_spaces_in_names() {
        assert!(m("/dökümanlar/", "dökümanlar/ü ber.md"));
        assert!(m("*.ü", "a/b.ü"));
        assert!(m("/emoji 🎉/*", "emoji 🎉/x"));
    }

    #[test]
    fn many_stars_against_a_long_path_end_quickly() {
        let pattern = "/a*a*a*a*a*a*a*a*a*a*a*a*b";
        let path = "a".repeat(200);
        let started = std::time::Instant::now();
        assert!(!m(pattern, &path));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(Pattern::new(pattern).is_wild());
    }

    #[test]
    fn empty_paths_match_nothing() {
        assert!(!m("*", ""));
        assert!(!m("/", "a"));
    }
}
