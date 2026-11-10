//! Diff path parsing and fair per-file prompt budgets.
/// A file's part of a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff<'a> {
    pub path: String,
    /// From its `diff --git` line to the next one.
    pub text: &'a str,
    pub added: usize,
    pub removed: usize,
}

/// Why a file's changes aren't all in the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A lock file: long, generated, and saying little a message needs.
    LockFile,
    /// No room for any of it.
    TooLarge,
    /// Its start is there, the rest isn't.
    Cut,
}

/// A file whose changes aren't all in the prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftOut {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub why: Why,
}

/// A diff cut to size.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bounded {
    pub text: String,
    pub left_out: Vec<LeftOut>,
    /// How many files the whole diff has.
    pub files: usize,
}

/// The least of a file worth sending when it can't all go: less says nothing.
pub(super) const MIN_PART: usize = 600;

/// The most files listed as left out; the rest are counted.
pub(super) const MAX_LISTED: usize = 100;

/// The files `diff` (`git diff` output, `a/` and `b/` prefixes) is about, in its order.
pub fn split_files(diff: &str) -> Vec<FileDiff<'_>> {
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            starts.push(at);
        }
        at += line.len();
    }
    // Anything before the first file (there shouldn't be) goes with it.
    if let Some(first) = starts.first_mut() {
        *first = 0;
    }
    let ends = starts.iter().skip(1).copied().chain([diff.len()]);
    starts
        .iter()
        .zip(ends)
        .map(|(&from, to)| file_diff(&diff[from..to]))
        .collect()
}

pub(super) fn file_diff(text: &str) -> FileDiff<'_> {
    let (mut added, mut removed) = (0, 0);
    let (mut old, mut new) = (None, None);
    let mut in_hunk = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
        } else if in_hunk {
            if line.starts_with('+') {
                added += 1;
            } else if line.starts_with('-') {
                removed += 1;
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            new = Some(p);
        } else if let Some(p) = line.strip_prefix("--- ") {
            old = Some(p);
        }
    }
    let named = |p: Option<&str>, prefix: &str| {
        let p = unquote(p?);
        p.strip_prefix(prefix).map(str::to_owned)
    };
    let path = named(new, "b/")
        .or_else(|| named(old, "a/"))
        .unwrap_or_else(|| header_path(text));
    FileDiff {
        path,
        text,
        added,
        removed,
    }
}

/// The path of a file with no `---` / `+++` lines (binary, a mode change, a pure rename):
/// from its `diff --git a/x b/x` line.
/// A rename or copy says its new name on a line of its own; else the header names the file
/// twice, `a/X b/X`, which is split in the middle (a name may hold ` b/` itself).
pub(super) fn header_path(text: &str) -> String {
    let named = text.lines().find_map(|l| {
        l.strip_prefix("rename to ")
            .or_else(|| l.strip_prefix("copy to "))
    });
    if let Some(name) = named {
        return unquote(name);
    }
    let first = text.lines().next().unwrap_or_default();
    let rest = first.strip_prefix("diff --git ").unwrap_or(first);
    if rest.len() % 2 == 1 {
        let half = rest.len() / 2;
        if rest.is_char_boundary(half) && rest[half..].starts_with(' ') {
            let (old, new) = (&rest[..half], &rest[half + 1..]);
            if let (Some(o), Some(n)) = (old.strip_prefix("a/"), new.strip_prefix("b/"))
                && o == n
            {
                return n.to_owned();
            }
        }
    }
    match rest.rfind(" b/").or_else(|| rest.rfind(" \"b/")) {
        Some(i) => {
            let name = unquote(&rest[i + 1..]);
            name.strip_prefix("b/").unwrap_or(&name).to_owned()
        }
        None => rest.to_owned(),
    }
}

/// A path as git prints it: a quoted one (a name with `"`, `\`, a tab or a newline) with its
/// quotes taken off and its escapes read.
pub(super) fn unquote(p: &str) -> String {
    let p = p.trim_end_matches(['\t', '\r']);
    match p.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
        Some(inner) => unescape(inner),
        None => p.to_owned(),
    }
}

/// `\"`, `\\`, `\t`, `\n` and octal bytes (`\303\274`) as git writes them in a quoted name.
pub(super) fn unescape(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let mut it = s.bytes().peekable();
    while let Some(b) = it.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match it.next() {
            Some(b't') => bytes.push(b'\t'),
            Some(b'n') => bytes.push(b'\n'),
            Some(b'r') => bytes.push(b'\r'),
            Some(d @ b'0'..=b'7') => {
                let mut value = u32::from(d - b'0');
                for _ in 0..2 {
                    match it.peek() {
                        Some(&d @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(d - b'0');
                            it.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether `path` is a lock file a package manager writes.
pub fn is_lock_file(path: &str) -> bool {
    const LOCKS: [&str; 12] = [
        "Cargo.lock",
        "package-lock.json",
        "npm-shrinkwrap.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "bun.lock",
        "poetry.lock",
        "uv.lock",
        "Gemfile.lock",
        "composer.lock",
        "go.sum",
    ];
    let name = path.rsplit('/').next().unwrap_or(path);
    LOCKS.contains(&name)
}

/// `diff` in at most `max` bytes. Each file gets a fair share: the small ones whole, the large
/// ones what is left divided between them (their start, cut at a line), those that get too
/// little none. Lock files are only named.
pub fn bound(diff: &str, max: usize) -> Bounded {
    let files = split_files(diff);
    let candidates: Vec<usize> = (0..files.len())
        .filter(|&i| !is_lock_file(&files[i].path))
        .collect();
    let allotted = shares(&files, &candidates, max);
    let mut out = Bounded {
        files: files.len(),
        ..Bounded::default()
    };
    for (i, file) in files.iter().enumerate() {
        let give = allotted[i];
        let why = if is_lock_file(&file.path) {
            Some(Why::LockFile)
        } else if give >= file.text.len() {
            out.text.push_str(file.text);
            None
        } else if give >= MIN_PART {
            push_start(&mut out.text, file.text, give);
            Some(Why::Cut)
        } else {
            Some(Why::TooLarge)
        };
        if let Some(why) = why {
            out.left_out.push(LeftOut {
                path: file.path.clone(),
                added: file.added,
                removed: file.removed,
                why,
            });
        }
    }
    out
}

/// How many bytes each file may have: `max` shared out among `candidates`, the smallest
/// first, each taking at most an equal share of what is left.
pub(super) fn shares(files: &[FileDiff<'_>], candidates: &[usize], max: usize) -> Vec<usize> {
    let mut allotted = vec![0; files.len()];
    let mut order = candidates.to_vec();
    order.sort_by_key(|&i| files[i].text.len());
    let mut left = max;
    let mut waiting = order.len();
    for i in order {
        let give = files[i].text.len().min(left / waiting.max(1));
        allotted[i] = give;
        left -= give;
        waiting -= 1;
    }
    // A share too small to send any of is given back; what that frees goes to whole files,
    // in their order, while they fit.
    for &i in candidates {
        let size = files[i].text.len();
        if allotted[i] < size && allotted[i] < MIN_PART {
            left += allotted[i];
            allotted[i] = 0;
        }
    }
    for &i in candidates {
        let size = files[i].text.len();
        if allotted[i] == 0 && size <= left {
            allotted[i] = size;
            left -= size;
        }
    }
    allotted
}

/// The start of `text`, at most `room` bytes of it cut after a whole line, and a line saying
/// it goes on.
pub(super) fn push_start(out: &mut String, text: &str, room: usize) {
    let mut end = room.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let end = text[..end].rfind('\n').map_or(end, |nl| nl + 1);
    let kept = &text[..end];
    let more = text[end..].lines().count();
    out.push_str(kept);
    if !kept.ends_with('\n') && !kept.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "[… {more} more line{} of this file left out]\n",
        plural(more)
    ));
}

pub(super) fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// What was left out, for the prompt; empty when nothing was.
pub fn left_out_text(left_out: &[LeftOut]) -> String {
    let mut whole: Vec<&LeftOut> = left_out.iter().filter(|l| l.why != Why::Cut).collect();
    let cut: Vec<&LeftOut> = left_out.iter().filter(|l| l.why == Why::Cut).collect();
    let mut s = String::new();
    let line = |l: &LeftOut| {
        let why = match l.why {
            Why::LockFile => ": a lock file",
            Why::TooLarge => ": too large",
            Why::Cut => "",
        };
        format!("- {} (+{} −{}){why}\n", l.path, l.added, l.removed)
    };
    if !whole.is_empty() {
        s.push_str("These files also changed; their diff is left out to keep this short:\n");
        let more = whole.len().saturating_sub(MAX_LISTED);
        whole.truncate(MAX_LISTED);
        for l in whole {
            s.push_str(&line(l));
        }
        if more > 0 {
            s.push_str(&format!("- and {more} more file{}\n", plural(more)));
        }
    }
    if !cut.is_empty() {
        s.push_str("These files' diffs are cut short (only their start is below):\n");
        for l in cut.iter().take(MAX_LISTED) {
            s.push_str(&line(l));
        }
        let more = cut.len().saturating_sub(MAX_LISTED);
        if more > 0 {
            s.push_str(&format!("- and {more} more file{}\n", plural(more)));
        }
    }
    s
}
