//! Reading `git show --remerge-diff` output and `git --version`.

use super::{Remerge, RemergeFile};

/// `(major, minor)` of `git --version` output: `git version 2.43.0`, `2.39.3 (Apple Git-145)`,
/// `2.41.0.windows.1`.
pub fn parse_git_version(text: &str) -> Option<(u32, u32)> {
    let rest = text.trim().strip_prefix("git version ")?;
    let mut parts = rest.split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Whether `line` (a line of a file, without the diff's `+`/`-` column) is a conflict marker
/// git writes: `<<<<<<< ours`, `|||||||`, `=======`, `>>>>>>> theirs`.
pub fn is_conflict_marker(line: &str) -> bool {
    let line = line.trim_end_matches('\r');
    ["<<<<<<<", "|||||||", ">>>>>>>"]
        .iter()
        .any(|m| line == *m || line.strip_prefix(m).is_some_and(|r| r.starts_with(' ')))
        || line == "======="
}

/// The files of a remerge diff, in git's order.
pub fn parse_remerge(text: &str) -> Remerge {
    let mut files = Vec::new();
    let mut current: Option<Section> = None;
    for line in text.split_terminator('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            files.extend(current.take().map(Section::finish));
            current = Some(Section::new(rest));
        } else if let Some(section) = current.as_mut() {
            section.line(line);
        }
    }
    files.extend(current.map(Section::finish));
    Remerge { files }
}

/// One file's part of the diff, being read.
struct Section {
    file: RemergeFile,
    /// The paths of the `diff --git` line, when they could be told apart.
    header_paths: Option<(String, String)>,
    old: Option<String>,
    new: Option<String>,
    in_hunk: bool,
}

impl Section {
    fn new(header: &str) -> Self {
        Section {
            file: RemergeFile {
                status: 'M',
                ..Default::default()
            },
            header_paths: header_paths(header),
            old: None,
            new: None,
            in_hunk: false,
        }
    }

    fn line(&mut self, line: &str) {
        if line.starts_with("@@") {
            self.in_hunk = true;
            return;
        }
        if self.in_hunk {
            self.body_line(line);
        } else {
            self.header_line(line.trim_end_matches('\r'));
        }
    }

    fn body_line(&mut self, line: &str) {
        if line.starts_with('+') {
            self.file.added += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            self.file.removed += 1;
            if is_conflict_marker(text) {
                self.file.resolves_markers = true;
            }
        }
    }

    fn header_line(&mut self, line: &str) {
        if let Some(message) = line.strip_prefix("remerge ") {
            self.file.conflicts.push(message.to_owned());
        } else if line.starts_with("new file mode") {
            self.file.status = 'A';
        } else if line.starts_with("deleted file mode") {
            self.file.status = 'D';
        } else if let Some(path) = line.strip_prefix("rename from ") {
            self.file.status = 'R';
            self.old = Some(unquote(path));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            self.file.status = 'R';
            self.new = Some(unquote(path));
        } else if let Some(path) = line.strip_prefix("--- ") {
            self.old = self.old.take().or_else(|| side_path(path, "a/"));
        } else if let Some(path) = line.strip_prefix("+++ ") {
            self.new = self.new.take().or_else(|| side_path(path, "b/"));
        }
    }

    fn finish(self) -> RemergeFile {
        let mut file = self.file;
        let (header_old, header_new) = self.header_paths.unzip();
        let new = self.new.or(header_new);
        let old = self.old.or(header_old);
        // A deleted file is known by the name it had.
        file.path = new.clone().or_else(|| old.clone()).unwrap_or_default();
        file.old_path = old.filter(|old| Some(old) != new.as_ref() && new.is_some());
        file
    }
}

/// The path of a `---` / `+++` line (`/dev/null` for no file), its `prefix` taken off. git ends
/// a name with a space in it with a tab.
fn side_path(text: &str, prefix: &str) -> Option<String> {
    let text = text.strip_suffix('\t').unwrap_or(text);
    if text == "/dev/null" {
        return None;
    }
    let path = unquote(text);
    Some(
        path.strip_prefix(prefix)
            .map_or(path.clone(), str::to_owned),
    )
}

/// The two paths of a `diff --git a/x b/y` line. Unquoted, a space can't be told from the one
/// between them, unless both are the same path, as they are in all but a rename (whose names
/// come from its `rename` lines anyway).
pub(super) fn header_paths(text: &str) -> Option<(String, String)> {
    if text.starts_with('"') {
        let (old, rest) = quoted(text)?;
        let rest = rest.strip_prefix(' ')?;
        let new = if rest.starts_with('"') {
            quoted(rest)?.0
        } else {
            rest.to_owned()
        };
        return Some((
            old.strip_prefix("a/")?.to_owned(),
            new.strip_prefix("b/")?.to_owned(),
        ));
    }
    if let Some(at) = text.find(" \"") {
        let (new, _) = quoted(&text[at + 1..])?;
        return Some((
            text[..at].strip_prefix("a/")?.to_owned(),
            new.strip_prefix("b/")?.to_owned(),
        ));
    }
    // `a/P b/P`: the middle space is in the middle.
    let half = text.len().checked_sub(1)? / 2;
    let (old, new) = (text.get(..half)?, text.get(half + 1..)?);
    let (old, new) = (old.strip_prefix("a/")?, new.strip_prefix("b/")?);
    (old == new && text.as_bytes().get(half) == Some(&b' '))
        .then(|| (old.to_owned(), new.to_owned()))
}

/// A name git may have quoted (`"a/q\"x"`), as it is.
pub(super) fn unquote(text: &str) -> String {
    match quoted(text) {
        Some((name, "")) => name,
        _ => text.to_owned(),
    }
}

/// The C-style quoted string `text` starts with, unescaped, and what follows it. Octal escapes
/// are bytes, so a quoted name outside ASCII comes back whole.
fn quoted(text: &str) -> Option<(String, &str)> {
    let mut bytes = Vec::new();
    let mut chars = text.strip_prefix('"')?.char_indices();
    let body = &text[1..];
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                let name = String::from_utf8_lossy(&bytes).into_owned();
                return Some((name, &body[i + 1..]));
            }
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    'n' => bytes.push(b'\n'),
                    't' => bytes.push(b'\t'),
                    'r' => bytes.push(b'\r'),
                    'a' => bytes.push(7),
                    'b' => bytes.push(8),
                    'f' => bytes.push(12),
                    'v' => bytes.push(11),
                    '0'..='7' => {
                        let mut value = e.to_digit(8)?;
                        for _ in 0..2 {
                            let (_, d) = chars.next()?;
                            value = value * 8 + d.to_digit(8)?;
                        }
                        bytes.push(u8::try_from(value).ok()?);
                    }
                    other => {
                        let mut buf = [0; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
            other => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    None
}
