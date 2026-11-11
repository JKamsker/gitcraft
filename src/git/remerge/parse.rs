//! Reading `git show --remerge-diff` output and `git --version`.

use super::{Remerge, RemergeFile, hunk_resolves};

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
/// git writes: `<<<<<<< ours`, `|||||||`, `=======`, `>>>>>>> theirs`, seven of the sign or
/// more (the `conflict-marker-size` attribute).
pub fn is_conflict_marker(line: &str) -> bool {
    let line = line.trim_end_matches('\r');
    let Some(sign) = line.chars().next().filter(|c| "<|=>".contains(*c)) else {
        return false;
    };
    let run = line.chars().take_while(|c| *c == sign).count();
    let rest = &line[run..];
    run >= 7 && (rest.is_empty() || (sign != '=' && rest.starts_with(' ')))
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
    let mut files = join_typechanges(files);
    blame_named_files(&mut files);
    Remerge {
        files: files.into_iter().map(Read::classify).collect(),
    }
}

/// A file read, and whether each of its hunks takes away conflict markers.
struct Read {
    file: RemergeFile,
    hunks: Vec<bool>,
}

impl Read {
    /// The file with its hunks counted as resolving a conflict or editing beyond the merge.
    fn classify(self) -> RemergeFile {
        let mut file = self.file;
        let any_marked = self.hunks.contains(&true);
        for marked in self.hunks {
            if hunk_resolves(&file.conflicts, marked, any_marked) {
                file.resolving_hunks += 1;
            } else {
                file.edited_hunks += 1;
            }
        }
        file
    }
}

/// A file whose type a conflict changed (a link become a file) comes as a deletion and an
/// addition of the same path: one entry, as it opens as one diff.
fn join_typechanges(files: Vec<Read>) -> Vec<Read> {
    let mut joined: Vec<Read> = Vec::with_capacity(files.len());
    for read in files {
        match joined.last_mut() {
            Some(last) if last.file.path == read.file.path => {
                last.file.status = 'T';
                last.file.added += read.file.added;
                last.file.removed += read.file.removed;
                last.file.conflicts.extend(read.file.conflicts);
                last.hunks.extend(read.hunks);
            }
            _ => joined.push(read),
        }
    }
    joined
}

/// git says what a conflict over several paths (rename/rename…) was about under one of them
/// only: the others it names were changed resolving it, not edited beyond the merge.
fn blame_named_files(files: &mut [Read]) {
    let messages: Vec<String> = files
        .iter()
        .flat_map(|r| r.file.conflicts.clone())
        .collect();
    for read in files.iter_mut().filter(|r| r.file.conflicts.is_empty()) {
        read.file.conflicts = messages
            .iter()
            .filter(|m| names(m, &read.file.path))
            .cloned()
            .collect();
    }
}

/// Whether git's `message` names `path`, as a whole word.
pub(super) fn names(message: &str, path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    message.match_indices(path).any(|(at, _)| {
        let before = message[..at].chars().next_back();
        let after = &message[at + path.len()..];
        // A full stop ends the sentence, not the name (`x.txt.bak` isn't `x.txt`).
        let after = after
            .strip_prefix('.')
            .filter(|rest| rest.is_empty() || rest.starts_with(' '))
            .unwrap_or(after);
        matches!(before, None | Some(' ' | '(' | '\'' | '"'))
            && matches!(
                after.chars().next(),
                None | Some(' ' | ',' | ')' | ':' | '\'' | '"')
            )
    })
}

/// One file's part of the diff, being read.
struct Section {
    file: RemergeFile,
    /// The paths of the `diff --git` line, when they could be told apart.
    header_paths: Option<(String, String)>,
    old: Option<String>,
    new: Option<String>,
    in_hunk: bool,
    /// Whether each hunk read takes away conflict markers.
    hunks: Vec<bool>,
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
            hunks: Vec::new(),
        }
    }

    fn line(&mut self, line: &str) {
        if line.starts_with("@@") {
            self.in_hunk = true;
            self.hunks.push(false);
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
            if is_conflict_marker(text)
                && let Some(marked) = self.hunks.last_mut()
            {
                *marked = true;
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

    fn finish(self) -> Read {
        let mut file = self.file;
        let (header_old, header_new) = self.header_paths.unzip();
        let new = self.new.or(header_new);
        let old = self.old.or(header_old);
        // A deleted file is known by the name it had.
        file.path = new.clone().or_else(|| old.clone()).unwrap_or_default();
        file.old_path = old.filter(|old| Some(old) != new.as_ref() && new.is_some());
        Read {
            file,
            hunks: self.hunks,
        }
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
