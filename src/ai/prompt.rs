//! The prompt a tool is given: the template filled in, the diff cut to the size the settings
//! allow (each file keeping a fair share, lock files left out), and what was left out listed by
//! name and line counts, so the answer can still mention it. Also what is made of the answer.

use super::settings::AiSettings;

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

/// A prompt ready to send, and what was left out of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    pub files: usize,
    pub left_out: usize,
}

/// The least of a file worth sending when it can't all go: less says nothing.
const MIN_PART: usize = 600;
/// The most files listed as left out; the rest are counted.
const MAX_LISTED: usize = 100;

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

fn file_diff(text: &str) -> FileDiff<'_> {
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
fn header_path(text: &str) -> String {
    let first = text.lines().next().unwrap_or_default();
    let rest = first.strip_prefix("diff --git ").unwrap_or(first);
    match rest.rfind(" b/").or_else(|| rest.rfind(" \"b/")) {
        Some(i) => unquote(&rest[i + 1..]).trim_start_matches("b/").to_owned(),
        None => rest.to_owned(),
    }
}

/// A path as git prints it, its quotes taken off (git quotes names with odd characters).
fn unquote(p: &str) -> &str {
    let p = p.trim_end_matches(['\t', '\r']);
    p.strip_prefix('"')
        .and_then(|p| p.strip_suffix('"'))
        .unwrap_or(p)
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
fn shares(files: &[FileDiff<'_>], candidates: &[usize], max: usize) -> Vec<usize> {
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
fn push_start(out: &mut String, text: &str, room: usize) {
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

fn plural(n: usize) -> &'static str {
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
    }
    s
}

/// Whether a commit summary follows Conventional Commits: `type(scope)!: text`.
pub fn is_conventional(subject: &str) -> bool {
    let Some((head, rest)) = subject.split_once(':') else {
        return false;
    };
    if !rest.starts_with(' ') || rest.trim().is_empty() {
        return false;
    }
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, Some(scope)),
        None => (head, None),
    };
    let kind_ok = (1..=12).contains(&kind.len()) && kind.chars().all(|c| c.is_ascii_lowercase());
    let scope_ok = scope.is_none_or(|s| {
        s.strip_suffix(')')
            .is_some_and(|inner| !inner.trim().is_empty() && !inner.contains(['(', ')']))
    });
    kind_ok && scope_ok
}

/// How many recent summaries are shown to the tool as examples.
const EXAMPLES: usize = 8;

/// What the prompt says of the repository's conventions, from its recent commit summaries
/// (newest first): Conventional Commits when most of them follow it, else their style.
pub fn conventions(subjects: &[String]) -> String {
    let subjects: Vec<&str> = subjects
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if subjects.is_empty() {
        return String::new();
    }
    let conventional = subjects.iter().filter(|s| is_conventional(s)).count();
    let mut s = if subjects.len() >= 3 && conventional * 10 >= subjects.len() * 6 {
        "This repository's history uses Conventional Commits: start the summary with a type \
         (feat, fix, docs, refactor, test, chore…), an optional scope in parentheses, then a \
         colon, like its recent summaries:\n"
            .to_owned()
    } else {
        "Match the style of this repository's recent summaries:\n".to_owned()
    };
    for subject in subjects.iter().take(EXAMPLES) {
        s.push_str(&format!("- {subject}\n"));
    }
    s
}

/// `template` with each `{name}` of `values` replaced, in one pass (a value holding `{diff}`
/// is left as it is). A placeholder alone on its line whose value is empty takes the line with
/// it. When the template has no `{diff}`, the diff goes at the end, so it is always sent.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut diff_used = false;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let found = values.iter().find(|(name, _)| {
            after
                .strip_prefix('{')
                .and_then(|a| a.strip_prefix(*name))
                .is_some_and(|a| a.starts_with('}'))
        });
        let Some((name, value)) = found else {
            out.push('{');
            rest = &after[1..];
            continue;
        };
        diff_used |= *name == "diff";
        rest = &after[name.len() + 2..];
        let alone = (out.is_empty() || out.ends_with('\n')) && rest.starts_with('\n');
        if value.is_empty() && alone {
            rest = &rest[1..];
        } else {
            out.push_str(value);
        }
    }
    out.push_str(rest);
    if !diff_used && let Some((_, diff)) = values.iter().find(|(n, _)| *n == "diff") {
        out.push_str("\n\n");
        out.push_str(diff);
    }
    out
}

/// The prompt for a commit message of `staged` (a diff), in a repository whose recent
/// summaries are `subjects`.
pub fn commit_prompt(settings: &AiSettings, staged: &str, subjects: &[String]) -> Prompt {
    let bounded = bound(staged, settings.max_diff_bytes());
    let text = fill(
        settings.commit_template(),
        &[
            ("conventions", &conventions(subjects)),
            ("left_out", &left_out_text(&bounded.left_out)),
            ("diff", &bounded.text),
        ],
    );
    Prompt {
        text,
        files: bounded.files,
        left_out: bounded.left_out.len(),
    }
}

/// What a review is about: a branch, the branch it goes into, and what it adds.
pub struct ReviewInput<'a> {
    pub branch: &'a str,
    pub base: &'a str,
    /// Its commits, a line each.
    pub commits: &'a str,
    pub diff: &'a str,
}

/// The prompt for a review of a branch.
pub fn review_prompt(settings: &AiSettings, input: &ReviewInput<'_>) -> Prompt {
    let bounded = bound(input.diff, settings.max_diff_bytes());
    let commits = if input.commits.trim().is_empty() {
        "(none)"
    } else {
        input.commits.trim_end()
    };
    let text = fill(
        settings.review_template(),
        &[
            ("branch", input.branch),
            ("base", input.base),
            ("commits", commits),
            ("left_out", &left_out_text(&bounded.left_out)),
            ("diff", &bounded.text),
        ],
    );
    Prompt {
        text,
        files: bounded.files,
        left_out: bounded.left_out.len(),
    }
}

/// A tool's answer as text: terminal colours and carriage returns out, and the code fence it
/// may have been wrapped in.
pub fn clean_answer(raw: &str) -> String {
    let plain = strip_escapes(raw).replace('\r', "");
    let mut text = plain.trim();
    if text.starts_with("```") && text.ends_with("```") && text.len() > 6 {
        let inner = &text[3..text.len() - 3];
        // The fence's language word, on its line.
        text = inner
            .split_once('\n')
            .map_or(inner, |(_, rest)| rest)
            .trim();
    }
    text.to_owned()
}

/// `s` without terminal escape sequences (colours, cursor moves, titles).
fn strip_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: up to a final byte.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A commit message answered by a tool, as the summary and the description of the commit box.
pub fn split_message(answer: &str) -> (String, String) {
    let mut lines = answer
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .peekable();
    // "Commit message:" before the message itself.
    if lines.peek().is_some_and(|first| {
        let first = first.trim().to_lowercase();
        first.ends_with(':') && first.contains("commit message")
    }) {
        lines.next();
        while lines.peek().is_some_and(|l| l.trim().is_empty()) {
            lines.next();
        }
    }
    let summary = lines.next().unwrap_or_default().trim();
    let summary = summary.trim_matches(|c| c == '`' || c == '"').trim();
    let body: Vec<&str> = lines.collect();
    let body = body.join("\n");
    (summary.to_owned(), body.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, added: usize) -> String {
        let mut s = format!(
            "diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n@@ -1 +1,{added} @@\n"
        );
        for i in 0..added {
            s.push_str(&format!("+line {i} of {path}\n"));
        }
        s
    }

    #[test]
    fn a_diff_is_split_by_file_with_counts() {
        let diff = format!(
            "{}{}diff --git a/old.txt b/old.txt\ndeleted file mode 100644\n--- a/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-+++ b\n",
            file("src/a.rs", 2),
            file("docs/b c.md", 1)
        );
        let files = split_files(&diff);
        let seen: Vec<(&str, usize, usize)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.added, f.removed))
            .collect();
        assert_eq!(
            seen,
            [("src/a.rs", 2, 0), ("docs/b c.md", 1, 0), ("old.txt", 0, 2)]
        );
        assert_eq!(files.iter().map(|f| f.text).collect::<String>(), diff);
    }

    #[test]
    fn odd_names_are_read_from_the_diff() {
        // git quotes names with a tab, a quote or (with core.quotePath) any non-ASCII.
        let quoted = "diff --git \"a/t\\tb\" \"b/t\\tb\"\n--- \"a/t\\tb\"\n+++ \"b/t\\tb\"\n@@ -1 +1 @@\n-x\n+y\n";
        assert_eq!(split_files(quoted)[0].path, "t\\tb");
        let unicode = file("ünï cødé/файл.txt", 1);
        assert_eq!(split_files(&unicode)[0].path, "ünï cødé/файл.txt");
        // A binary file has no --- / +++ lines.
        let binary = "diff --git a/img/a b.png b/img/a b.png\nnew file mode 100644\nBinary files /dev/null and b/img/a b.png differ\n";
        assert_eq!(split_files(binary)[0].path, "img/a b.png");
        assert!(split_files("").is_empty());
    }

    #[test]
    fn a_small_diff_goes_whole() {
        let diff = format!("{}{}", file("a", 3), file("b", 3));
        let b = bound(&diff, 64 * 1024);
        assert_eq!(b.text, diff);
        assert!(b.left_out.is_empty());
        assert_eq!(b.files, 2);
    }

    #[test]
    fn a_huge_file_is_cut_and_the_small_ones_stay_whole() {
        let small = file("small.rs", 5);
        let huge = file("huge.rs", 50_000);
        let diff = format!("{huge}{small}");
        let max = 8 * 1024;
        let b = bound(&diff, max);
        assert!(b.text.len() <= max + 100, "{}", b.text.len());
        assert!(b.text.contains(&small));
        assert!(b.text.contains("more lines of this file left out"));
        assert_eq!(
            b.left_out,
            [LeftOut {
                path: "huge.rs".to_owned(),
                added: 50_000,
                removed: 0,
                why: Why::Cut
            }]
        );
        // Cut after a whole line.
        let cut_at = b.text.find("[…").unwrap_or_default();
        assert!(b.text[..cut_at].ends_with('\n'));
    }

    #[test]
    fn many_files_past_the_limit_are_listed() {
        let diff: String = (0..400).map(|i| file(&format!("f{i}.txt"), 20)).collect();
        let b = bound(&diff, 4096);
        assert!(b.text.len() <= 4096);
        assert!(b.left_out.iter().any(|l| l.why == Why::TooLarge));
        let listed = left_out_text(&b.left_out);
        assert!(listed.contains(": too large\n"), "{listed}");
        // As many whole files as fit go, in their order.
        assert!(b.text.contains("+++ b/f0.txt"), "{}", b.text);
        assert!(listed.contains("more files"), "{listed}");
        assert!(listed.lines().count() <= MAX_LISTED + 3);
    }

    #[test]
    fn lock_files_are_only_named() {
        let diff = format!(
            "{}{}",
            file("Cargo.lock", 3),
            file("web/package-lock.json", 3)
        );
        let b = bound(&diff, 64 * 1024);
        assert!(b.text.is_empty());
        assert!(b.left_out.iter().all(|l| l.why == Why::LockFile));
        assert!(left_out_text(&b.left_out).contains("- Cargo.lock (+3 −0): a lock file"));
        assert!(!is_lock_file("src/Cargo.locker"));
    }

    #[test]
    fn multibyte_text_is_cut_on_a_character() {
        let mut diff = file("emoji.txt", 0);
        for _ in 0..2000 {
            diff.push_str("+😀😀😀 ünïcødé\n");
        }
        for max in [4000, 4001, 4002, 4003] {
            let b = bound(&diff, max);
            assert!(b.text.len() <= max + 60);
            assert!(b.text.contains("😀"));
        }
    }

    #[test]
    fn nothing_is_listed_when_nothing_is_left_out() {
        assert_eq!(left_out_text(&[]), "");
    }

    #[test]
    fn conventional_summaries_are_recognised() {
        for yes in [
            "feat: add x",
            "fix(ui): y",
            "refactor!: z",
            "feat(api)!: drop v1",
        ] {
            assert!(is_conventional(yes), "{yes}");
        }
        for no in [
            "Add x",
            "Merge branch 'a'",
            "feat:x",
            "Fix: y",
            "feat(): y",
            "feat: ",
            "a really long kind: x",
            "",
        ] {
            assert!(!is_conventional(no), "{no}");
        }
    }

    #[test]
    fn conventions_follow_the_history() {
        let owned = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let conventional = conventions(&owned(&["feat: a", "fix(x): b", "docs: c", "Add d"]));
        assert!(
            conventional.contains("Conventional Commits"),
            "{conventional}"
        );
        assert!(conventional.contains("- fix(x): b"));
        let plain = conventions(&owned(&["Add a", "Fix b", "feat: c"]));
        assert!(!plain.contains("Conventional"), "{plain}");
        assert!(plain.contains("- Add a"));
        // Too few to tell.
        assert!(!conventions(&owned(&["feat: a"])).contains("Conventional"));
        assert_eq!(conventions(&[]), "");
        let many: Vec<String> = (0..30).map(|i| format!("feat: {i}")).collect();
        assert_eq!(conventions(&many).lines().count(), EXAMPLES + 1);
    }

    #[test]
    fn a_template_is_filled_in_one_pass() {
        let filled = fill(
            "A {x}\n{empty}\nB {diff} {unknown}",
            &[("x", "{diff}"), ("empty", ""), ("diff", "D")],
        );
        assert_eq!(filled, "A {diff}\nB D {unknown}");
        // No {diff}: it comes at the end all the same.
        assert_eq!(fill("Describe it.", &[("diff", "D")]), "Describe it.\n\nD");
        // An empty value not alone on its line just goes.
        assert_eq!(fill("a{e}b", &[("e", "")]), "ab");
        assert_eq!(fill("{", &[]), "{");
        assert_eq!(fill("ü{x}ü", &[("x", "€")]), "ü€ü");
    }

    #[test]
    fn the_commit_prompt_holds_the_diff_conventions_and_what_was_left_out() {
        let settings = AiSettings {
            max_diff_kb: 4,
            ..AiSettings::default()
        };
        let diff = format!("{}{}", file("a.rs", 2), file("big.rs", 10_000));
        let subjects: Vec<String> = ["feat: a", "fix: b", "chore: c"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let p = commit_prompt(&settings, &diff, &subjects);
        assert!(p.text.contains("Conventional Commits"));
        assert!(p.text.contains("+line 0 of a.rs"));
        assert!(p.text.contains("- big.rs (+10000 −0)"));
        assert!(!p.text.contains("{diff}") && !p.text.contains("{left_out}"));
        assert_eq!((p.files, p.left_out), (2, 1));
        assert!(p.text.len() < 6 * 1024);
    }

    #[test]
    fn the_review_prompt_names_the_branches() {
        let settings = AiSettings::default();
        let diff = file("x.rs", 1);
        let p = review_prompt(
            &settings,
            &ReviewInput {
                branch: "feat/ünï",
                base: "main",
                commits: "",
                diff: &diff,
            },
        );
        assert!(p.text.contains("branch feat/ünï adds to main"));
        assert!(p.text.contains("Commits:\n(none)"));
        assert!(p.text.contains("+line 0 of x.rs"));
    }

    #[test]
    fn answers_are_cleaned() {
        assert_eq!(
            clean_answer("\u{1b}[1;32mfeat: x\u{1b}[0m\r\n\r\nbody\r\n"),
            "feat: x\n\nbody"
        );
        assert_eq!(clean_answer("```text\nfix: y\n\nwhy\n```"), "fix: y\n\nwhy");
        assert_eq!(clean_answer("\u{1b}]0;title\u{7}ok"), "ok");
        assert_eq!(clean_answer("  \n "), "");
    }

    #[test]
    fn a_message_is_split_into_summary_and_description() {
        assert_eq!(
            split_message("feat: add x\n\nBecause y.\nAnd z.\n"),
            ("feat: add x".to_owned(), "Because y.\nAnd z.".to_owned())
        );
        assert_eq!(
            split_message("Commit message:\n\n`fix: ü`"),
            ("fix: ü".to_owned(), String::new())
        );
        assert_eq!(split_message(""), (String::new(), String::new()));
    }
}
