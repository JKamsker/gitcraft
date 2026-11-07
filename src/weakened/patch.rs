//! A unified diff of one or more files read into files and hunks: what the rules look at.

/// What a line of a hunk is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Context,
    Added,
    Removed,
}

/// A line of a hunk: its mark, its number on its side (the new file for added and context
/// lines, the old one for removed lines) and its text without the mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub mark: Mark,
    pub number: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hunk {
    /// Where the hunk starts in the old file, and how many of its lines it covers.
    pub old_start: u32,
    pub old_count: u32,
    pub lines: Vec<Line>,
}

impl Hunk {
    pub fn count(&self, mark: Mark) -> usize {
        self.lines.iter().filter(|l| l.mark == mark).count()
    }
}

/// One file of the patch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePatch {
    /// The new path (the old one for a deleted file).
    pub path: String,
    pub deleted: bool,
    pub added: bool,
    pub hunks: Vec<Hunk>,
}

/// Reads `text` (`git diff` / `git show` output with `a/` and `b/` prefixes) into its files.
pub fn parse(text: &str) -> Vec<FilePatch> {
    let mut files: Vec<FilePatch> = Vec::new();
    let (mut old, mut new) = (0u32, 0u32);
    let mut in_hunk = false;
    for raw in text.split_terminator('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix("diff --git ") {
            files.push(FilePatch {
                path: path_of_header(rest),
                ..FilePatch::default()
            });
            in_hunk = false;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with("@@") {
            in_hunk = true;
            (old, new) = hunk_start(line);
            file.hunks.push(Hunk {
                old_start: old.saturating_add(1),
                old_count: old_count(line),
                lines: Vec::new(),
            });
            continue;
        }
        if !in_hunk {
            header_line(file, line);
            continue;
        }
        body_line(file, line, &mut old, &mut new);
    }
    files
}

/// What a line before the first hunk says about the file: its paths, new or deleted.
fn header_line(file: &mut FilePatch, line: &str) {
    if line.starts_with("deleted file mode") {
        file.deleted = true;
    } else if line.starts_with("new file mode") {
        file.added = true;
    } else if let Some(p) = line.strip_prefix("+++ ") {
        if let Some(p) = strip_side(&unquote(p), "b/") {
            file.path = p;
        }
    } else if let Some(p) = line.strip_prefix("--- ")
        && file.path.is_empty()
        && let Some(p) = strip_side(&unquote(p), "a/")
    {
        file.path = p;
    } else if let Some(p) = line.strip_prefix("rename to ") {
        file.path = unquote(p);
    }
}

fn body_line(file: &mut FilePatch, line: &str, old: &mut u32, new: &mut u32) {
    let Some(hunk) = file.hunks.last_mut() else {
        return;
    };
    let (mark, text) = match line.chars().next() {
        Some('+') => (Mark::Added, &line[1..]),
        Some('-') => (Mark::Removed, &line[1..]),
        Some(' ') => (Mark::Context, &line[1..]),
        // `\ No newline at end of file`, or an empty context line some tools write.
        Some('\\') => return,
        None => (Mark::Context, ""),
        Some(_) => return,
    };
    let number = match mark {
        Mark::Added => {
            *new = new.saturating_add(1);
            *new
        }
        Mark::Removed => {
            *old = old.saturating_add(1);
            *old
        }
        Mark::Context => {
            *old = old.saturating_add(1);
            *new = new.saturating_add(1);
            *new
        }
    };
    hunk.lines.push(Line {
        mark,
        number,
        text: text.to_owned(),
    });
}

/// The line numbers just before a hunk's first line, old and new: `@@ -a,b +c,d @@`.
fn hunk_start(header: &str) -> (u32, u32) {
    let mut parts = header.split_whitespace().skip(1);
    let start = |part: Option<&str>, sign: char| {
        part.and_then(|p| p.strip_prefix(sign))
            .and_then(|p| p.split(',').next())
            .and_then(|n| n.parse::<u32>().ok())
            .map_or(0, |n| n.saturating_sub(1))
    };
    let old = start(parts.next(), '-');
    let new = start(parts.next(), '+');
    (old, new)
}

/// How many old lines `@@ -a,b +c,d @@` covers: `b`, or 1 when it isn't given.
fn old_count(header: &str) -> u32 {
    header
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.strip_prefix('-'))
        .map_or(0, |p| {
            p.split_once(',')
                .map_or(Some(1), |(_, n)| n.parse().ok())
                .unwrap_or(0)
        })
}

/// The path of `diff --git a/x b/x` (its new side), until `+++` names it for sure.
fn path_of_header(rest: &str) -> String {
    if rest.starts_with('"') {
        // `"a/x" "b/y"`: the last quoted part.
        if let Some(at) = rest.rfind(" \"") {
            return strip_side(&unquote(&rest[at + 1..]), "b/").unwrap_or_default();
        }
    }
    // Same name both sides (the common case): `a/<p> b/<p>`, a space possibly in `<p>`.
    let half = rest.len().saturating_sub(1) / 2;
    match (rest.get(..half), rest.get(half + 1..)) {
        (Some(a), Some(b)) if a.strip_prefix("a/") == b.strip_prefix("b/") => {
            b.strip_prefix("b/").unwrap_or(b).to_owned()
        }
        _ => rest
            .rsplit_once(" b/")
            .map(|(_, p)| p.to_owned())
            .unwrap_or_default(),
    }
}

/// `p` without its `a/` or `b/`; `None` for `/dev/null`.
fn strip_side(p: &str, side: &str) -> Option<String> {
    if p == "/dev/null" {
        return None;
    }
    Some(p.strip_prefix(side).unwrap_or(p).to_owned())
}

/// A path as git prints it, its C-style quotes undone (`"tab\there"`, `"\303\251"`).
pub fn unquote(p: &str) -> String {
    let p = p.trim_end_matches('\t');
    let Some(inner) = p.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return p.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(d @ b'0'..=b'7') => {
                let mut n = u32::from(d - b'0');
                for _ in 0..2 {
                    match chars.peek() {
                        Some(&d @ b'0'..=b'7') => {
                            n = n * 8 + u32::from(d - b'0');
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(u8::try_from(n).unwrap_or(b'?'));
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
