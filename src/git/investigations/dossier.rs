//! Bounded committed-line history. No working file, index, filter or forge is read here.
//! A card is a real pinned file version; attribution and uncertain parent mappings are separate.
use super::*;
use std::collections::{BTreeSet, HashMap};
const FILE: usize = 2 * 1024 * 1024;
const TOTAL: usize = 32 * 1024 * 1024;
const LINES: usize = 20000;
const STEPS: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Seed {
    pub commit: String,
    pub path: String,
    pub line: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Attribution {
    pub commit: String,
    pub path: String,
    pub line: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Card {
    pub at: Seed,
    pub text: String,
    pub file_oid: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub attribution: Attribution,
    pub note: String,
    pub whitespace_only: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Dossier {
    pub start: Seed,
    pub cards: Vec<Card>,
    pub stopped: String,
    pub continuation: Option<Seed>,
    pub alternatives: Vec<Seed>,
}
struct FileVersion {
    oid: String,
    text: String,
}
struct Trace<'a> {
    reader: Reader<'a>,
    files: HashMap<(String, String), std::sync::Arc<FileVersion>>,
    blobs: HashMap<String, std::sync::Arc<FileVersion>>,
    bytes: usize,
}
fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains(['\0', '\\'])
        && path.split('/').all(|part| {
            !part.is_empty() && !matches!(part, "." | "..") && !part.eq_ignore_ascii_case(".git")
        })
}
fn one_line(text: &str, line: usize) -> Result<&str, String> {
    if !(1..=LINES).contains(&line) {
        return Err("Line history is limited to20000 source lines".into());
    }
    text.split_terminator('\n')
        .nth(line - 1)
        .ok_or_else(|| "The selected line no longer exists at the pinned revision".into())
}
fn normal(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
pub(crate) fn quoted_path(value: &str) -> Result<String, String> {
    if !value.starts_with('"') {
        return if path_ok(value) {
            Ok(value.into())
        } else {
            Err("Invalid Git history path".into())
        };
    }
    if !value.ends_with('"') || value.len() < 2 {
        return Err("Truncated quoted Git path".into());
    }
    let mut out = Vec::new();
    let bytes = &value.as_bytes()[1..value.len() - 1];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let b = *bytes.get(i).ok_or("Truncated path escape")?;
        i += 1;
        match b {
            b'\\' | b'"' => out.push(b),
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'a' => out.push(7),
            b'0'..=b'7' => {
                let rest = bytes.get(i..i + 2).ok_or("Truncated octal path")?;
                if !rest.iter().all(|b| matches!(b, b'0'..=b'7')) {
                    return Err("Invalid octal path".into());
                }
                let n = u16::from(b - b'0') * 64
                    + u16::from(rest[0] - b'0') * 8
                    + u16::from(rest[1] - b'0');
                out.push(u8::try_from(n).map_err(|_| "Invalid octal byte")?);
                i += 2;
            }
            _ => return Err("Unsupported Git path escape".into()),
        }
    }
    let path = String::from_utf8(out).map_err(|_| "Non-UTF-8 history paths are not traced")?;
    if !path_ok(&path) {
        return Err("Unsafe history path".into());
    }
    Ok(path)
}
fn blame_frame(out: &[u8]) -> Result<Attribution, String> {
    let text = std::str::from_utf8(out).map_err(|_| "Blame returned non-UTF-8 metadata")?;
    let mut lines = text.lines();
    let first = lines.next().ok_or("Empty line blame")?;
    if first.len() > 256 {
        return Err("Oversized blame header".into());
    }
    let header: Vec<_> = first.split_whitespace().collect();
    let [commit, line, _, _] = header.as_slice() else {
        return Err("Invalid single-line blame header".into());
    };
    if !oid(commit) {
        return Err("Invalid blame object ID".into());
    }
    let line = line
        .parse::<usize>()
        .map_err(|_| "Invalid original line number")?;
    if !(1..=LINES).contains(&line) {
        return Err("Original line exceeds history limits".into());
    }
    let mut path = None;
    let mut records = 0;
    for line in lines {
        if let Some(value) = line.strip_prefix("filename ") {
            if path.is_some() {
                return Err("Duplicate blame filename".into());
            }
            path = Some(quoted_path(value)?);
        } else if line.starts_with('\t') {
            records += 1;
        }
    }
    if records != 1 {
        return Err("Blame did not return exactly one line".into());
    }
    Ok(Attribution {
        commit: (*commit).into(),
        path: path.ok_or("Missing blame filename")?,
        line,
    })
}
/// Unique equal text wins; otherwise use two unique context anchors and a small,
/// same-sized replacement block. Repeated/unequal blocks stop, rather than choosing a line.
fn previous_line(
    current: &str,
    parent: &str,
    line: usize,
) -> Result<Option<(usize, &'static str)>, String> {
    let now: Vec<_> = current.split_terminator('\n').collect();
    let old: Vec<_> = parent.split_terminator('\n').collect();
    if now.len() > LINES || old.len() > LINES {
        return Err("File has over20000 lines".into());
    }
    let target = normal(now.get(line - 1).ok_or("Invalid current line")?);
    if current == parent {
        return Ok(Some((
            line,
            "Identical raw file version; exact line position",
        )));
    }
    let old_normal: Vec<_> = old.iter().map(|text| normal(text)).collect();
    let equal: Vec<_> = old_normal
        .iter()
        .enumerate()
        .filter(|(_, text)| **text == target)
        .map(|(i, _)| i)
        .collect();
    if equal.len() == 1 {
        return Ok(Some((
            equal[0] + 1,
            "Unique whitespace-normalized line match",
        )));
    }
    let anchor = |at: usize| -> Option<usize> {
        let value = normal(now[at]);
        if value.is_empty() {
            return None;
        }
        let mut found = old_normal
            .iter()
            .enumerate()
            .filter(|(_, text)| **text == value)
            .map(|(i, _)| i);
        let first = found.next()?;
        found.next().is_none().then_some(first)
    };
    let at = line - 1;
    let left = (at.saturating_sub(20)..at)
        .rev()
        .find_map(|i| anchor(i).map(|j| (i + 1, j + 1)))
        .unwrap_or((0, 0));
    let right = (at + 1..now.len().min(at + 21))
        .find_map(|i| anchor(i).map(|j| (i, j)))
        .unwrap_or((now.len(), old.len()));
    if right.0 < left.0 || right.1 < left.1 {
        return Ok(None);
    }
    let a = right.0 - left.0;
    let b = right.1 - left.1;
    if a == 1 && b == 1 && at >= left.0 && at < right.0 {
        let candidate = left.1 + (at - left.0);
        if equal.is_empty() || equal.contains(&candidate) {
            return Ok(Some((
                candidate + 1,
                "Parent line aligned by unique surrounding context; inspect the version",
            )));
        }
    }
    Ok(None)
}
impl<'a> Trace<'a> {
    fn file(
        &mut self,
        commit: &str,
        path: &str,
    ) -> Result<Option<std::sync::Arc<FileVersion>>, String> {
        let key = (commit.to_owned(), path.to_owned());
        if let Some(file) = self.files.get(&key) {
            return Ok(Some(file.clone()));
        }
        let out = self.reader.run(
            &[
                "--literal-pathspecs",
                "ls-tree",
                "-z",
                "--full-tree",
                commit,
                "--",
                path,
            ],
            vec![],
            &[0],
        )?;
        if out.is_empty() {
            return Ok(None);
        }
        let mut records = out.split(|b| *b == 0).filter(|s| !s.is_empty());
        let record = records.next().ok_or("Missing tree record")?;
        if records.next().is_some() {
            return Err("Line history expected one exact file".into());
        }
        let split = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("Invalid tree record")?;
        if &record[split + 1..] != path.as_bytes() {
            return Err("Tree path did not match its literal request".into());
        }
        let fields: Vec<_> = std::str::from_utf8(&record[..split])
            .map_err(|_| "Invalid tree metadata")?
            .split_whitespace()
            .collect();
        let [mode, kind, id] = fields.as_slice() else {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        };
        if !matches!(*mode, "100644" | "100755") || *kind != "blob" || !oid(id) {
            return Err(
                "Line history only reads regular text blobs; links/submodules are not followed"
                    .into(),
            );
        }
        if let Some(file) = self.blobs.get(*id) {
            let file = file.clone();
            self.files.insert(key, file.clone());
            return Ok(Some(file));
        }
        let size = self.reader.run(&["cat-file", "-s", id], vec![], &[0])?;
        let size = std::str::from_utf8(&size)
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .ok_or(crate::i18n::text("backend-invalid-blob-size-924b45"))?;
        if size > FILE || self.bytes.saturating_add(size) > TOTAL {
            return Err("Line history reached its2MiB/file or32MiB snapshot budget".into());
        }
        let bytes = self.reader.run(&["cat-file", "blob", id], vec![], &[0])?;
        if bytes.len() != size || bytes.contains(&0) {
            return Err("Binary or incomplete file version cannot be traced".into());
        }
        let actual = self.reader.run(
            &["hash-object", "--no-filters", "--stdin"],
            bytes.clone(),
            &[0],
        )?;
        if actual.trim_ascii() != id.as_bytes() {
            return Err("Blob identity changed while tracing history".into());
        }
        let text =
            String::from_utf8(bytes).map_err(|_| "Non-UTF-8 file versions cannot be traced")?;
        if text.split_terminator('\n').count() > LINES {
            return Err("File has over20000 lines".into());
        }
        self.bytes += size;
        let file = std::sync::Arc::new(FileVersion {
            oid: (*id).into(),
            text,
        });
        self.blobs.insert((*id).into(), file.clone());
        self.files.insert(key, file.clone());
        Ok(Some(file))
    }
    fn parents(&self, commit: &str) -> Result<(Vec<String>, String, String, i64, String), String> {
        let out = self.reader.run(
            &[
                "show",
                "-s",
                "--no-show-signature",
                "--encoding=UTF-8",
                "--format=%P%x00%an%x00%ae%x00%at%x00%s%x00",
                commit,
                "--",
            ],
            vec![],
            &[0],
        )?;
        if out.len() > 16384 {
            return Err("Version metadata exceeds16KiB".into());
        }
        let fields: Vec<_> = out.split(|b| *b == 0).collect();
        let [parents, author, email, time, subject, tail] = fields.as_slice() else {
            return Err("Invalid version metadata".into());
        };
        if !tail.iter().all(u8::is_ascii_whitespace) {
            return Err("Trailing version metadata".into());
        }
        let text = |b: &[u8]| {
            String::from_utf8(b.to_vec()).map_err(|_| "Invalid UTF-8 version metadata".to_owned())
        };
        let parents: Vec<_> = text(parents)?
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if parents.len() > 8 || parents.iter().any(|id| !oid(id)) {
            return Err("Unsupported or malformed commit parent list".into());
        }
        Ok((
            parents,
            text(author)?,
            text(email)?,
            text(time)?.parse().map_err(|_| "Invalid author time")?,
            text(subject)?,
        ))
    }
    fn old_path(&self, parent: &str, current: &str, path: &str) -> Result<Option<String>, String> {
        let out = self.reader.run(
            &[
                "diff-tree",
                "-r",
                "--no-commit-id",
                "--name-status",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                "-M",
                "-C",
                "--find-copies-harder",
                "-l100",
                parent,
                current,
                "--",
            ],
            vec![],
            &[0],
        )?;
        let mut fields = out.split(|b| *b == 0).filter(|s| !s.is_empty());
        let mut seen = 0;
        while let Some(status) = fields.next() {
            seen += 1;
            if seen > 10000 {
                return Err("Rename detection exceeded10000 changed paths".into());
            }
            let first = fields.next().ok_or("Truncated rename record")?;
            let rename = matches!(status.first(), Some(b'R' | b'C'));
            let second = if rename {
                Some(fields.next().ok_or("Truncated rename destination")?)
            } else {
                None
            };
            if second.is_some_and(|new| new == path.as_bytes()) {
                let old =
                    std::str::from_utf8(first).map_err(|_| "Renamed history path is not UTF-8")?;
                if !path_ok(old) {
                    return Err("Unsafe renamed history path".into());
                }
                return Ok(Some(old.into()));
            }
            if !rename && first == path.as_bytes() && status.first() == Some(&b'A') {
                return Ok(None);
            }
        }
        Ok(Some(path.into()))
    }
    fn touch(&self, parent: &str, path: &str) -> Result<Option<String>, String> {
        let out = self.reader.run(
            &[
                "--literal-pathspecs",
                "log",
                "--no-show-signature",
                "--first-parent",
                "--max-count=1",
                "--format=%H",
                parent,
                "--",
                path,
            ],
            vec![],
            &[0],
        )?;
        let value = std::str::from_utf8(&out)
            .map_err(|_| "Invalid file history")?
            .trim();
        if value.is_empty() {
            Ok(None)
        } else if oid(value) {
            Ok(Some(value.into()))
        } else {
            Err("Invalid file-history object ID".into())
        }
    }
}
/// `revision` is already a graph/file snapshot ID, or HEAD for a deliberate committed view.
pub(crate) fn trace(
    repo: &Path,
    revision: &str,
    path: &str,
    line: usize,
    active: &dyn Fn() -> bool,
) -> Result<Dossier, String> {
    if !(oid(revision) || revision == "HEAD") || !path_ok(path) || !(1..=LINES).contains(&line) {
        return Err("Choose a valid committed file and line (up to20000 lines)".into());
    }
    let mut t = Trace {
        reader: Reader::new(repo, active),
        files: HashMap::new(),
        blobs: HashMap::new(),
        bytes: 0,
    };
    let resolved = t.reader.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        vec![],
        &[0],
    )?;
    let commit = std::str::from_utf8(&resolved)
        .map_err(|_| "Invalid pinned commit")?
        .trim()
        .to_owned();
    if !oid(&commit) {
        return Err("Invalid pinned history commit".into());
    }
    let start = Seed {
        commit,
        path: path.into(),
        line,
    };
    let mut cursor = start.clone();
    let mut result = Dossier {
        start,
        cards: Vec::new(),
        stopped: String::new(),
        continuation: None,
        alternatives: vec![],
    };
    let mut seen = BTreeSet::new();
    for step in 0..STEPS {
        if !t.reader.active() {
            return Err("Line history cancelled or exceeded120seconds".into());
        }
        if !seen.insert(cursor.clone()) {
            result.stopped = "History mapping returned to a previously visited line".into();
            break;
        }
        let file = t
            .file(&cursor.commit, &cursor.path)?
            .ok_or("Pinned history file is missing")?;
        let text = one_line(&file.text, cursor.line)?.to_owned();
        let range = format!("{},{}", cursor.line, cursor.line);
        let attribution = blame_frame(&t.reader.run(
            &[
                "-c",
                "i18n.logOutputEncoding=UTF-8",
                "blame",
                "-w",
                "-C",
                "-C",
                "-M",
                "--line-porcelain",
                "--no-textconv",
                "--no-ignore-revs-file",
                "-L",
                &range,
                &cursor.commit,
                "--",
                &cursor.path,
            ],
            vec![],
            &[0],
        )?)?;
        let origin = t
            .file(&attribution.commit, &attribution.path)?
            .ok_or("The attributed file is missing")?;
        let original = one_line(&origin.text, attribution.line)?;
        if normal(original) != normal(&text) {
            return Err("Blame attribution did not match the pinned raw line".into());
        }
        let (parents, author, email, time, subject) = t.parents(&cursor.commit)?;
        let mut mapped = Vec::new();
        for parent in &parents {
            if let Some(path) = t.old_path(parent, &cursor.commit, &cursor.path)?
                && let Some(old) = t.file(parent, &path)?
                && let Some((line, note)) = previous_line(&file.text, &old.text, cursor.line)?
            {
                mapped.push((
                    Seed {
                        commit: parent.clone(),
                        path,
                        line,
                    },
                    old,
                    note,
                ));
            }
        }
        // A partial copy may be below whole-file rename/copy similarity. Follow its
        // precise -C origin only after proving it belongs to an actual parent history.
        let mut copied = None;
        if mapped.is_empty() && attribution.commit != cursor.commit {
            for parent in &parents {
                let base = t.reader.run(
                    &["merge-base", &attribution.commit, parent],
                    vec![],
                    &[0, 1],
                )?;
                if base.trim_ascii() == attribution.commit.as_bytes() {
                    copied = Some(Seed {
                        commit: attribution.commit.clone(),
                        path: attribution.path.clone(),
                        line: attribution.line,
                    });
                    break;
                }
            }
        }
        let changed = mapped.first().is_none_or(|(seed, old, _)| {
            seed.path != cursor.path || one_line(&old.text, seed.line).ok() != Some(text.as_str())
        });
        if step == 0 || changed {
            let whitespace_only = parents.len() == 1
                && changed
                && mapped.first().is_some_and(|(seed, old, _)| {
                    seed.path == cursor.path
                        && normal(&old.text) == normal(&file.text)
                        && old.text != file.text
                });
            result.cards.push(Card {
                at: cursor.clone(),
                text,
                file_oid:file.oid.clone(),
                author,
                email,
                time,
                subject,
                attribution,
                note: if copied.is_some(){
                    "Git copy/move attribution verified in a parent history; whole-file patch mapping was unavailable".into()
                } else if step == 0 && !changed {
                    "Selected snapshot context; attribution names the last semantic edit".into()
                } else {
                    mapped
                        .first()
                        .map_or("No unique parent line mapping", |(_, _, note)| *note)
                        .into()
                },
                whitespace_only,
            });
        }
        if parents.is_empty() {
            result.stopped = "Reached a root or shallow history boundary".into();
            break;
        }
        if let Some(next) = copied {
            cursor = next;
            if step + 1 == STEPS {
                result.stopped =
                    "128 history steps reached; continue from the pinned earlier line".into();
                result.continuation = Some(cursor.clone());
            }
            continue;
        }
        if mapped.is_empty() {
            result.stopped="Line introduction or an ambiguous/unequal parent edit; no earlier line was guessed".into();
            break;
        }
        if parents.len() > 1 {
            result.alternatives = mapped.into_iter().map(|(seed, _, _)| seed).collect();
            result.stopped="Merge boundary: choose an explicitly mapped parent to continue; other histories may differ".into();
            break;
        }
        let (mut next, old, _) = mapped.remove(0);
        if let Some(touch) = t.touch(&next.commit, &next.path)?
            && touch != next.commit
            && let Some(previous) = t.file(&touch, &next.path)?
            && previous.oid == old.oid
        {
            next.commit = touch;
        }
        cursor = next;
        if step + 1 == STEPS {
            result.stopped =
                "128 history steps reached; continue from the pinned earlier line".into();
            result.continuation = Some(cursor.clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::investigations::tests::Repo;
    fn commit(r: &Repo, path: &str, body: &str, time: i64) -> Result<String, String> {
        std::fs::write(r.0.join(path), body).map_err(|e| e.to_string())?;
        r.git(&["add", "--", path])?;
        r.commit("line@test.invalid", time)
    }
    #[test]
    fn repeated_blame_follows_actual_versions_renames_and_formatting_without_index_or_work_writes()
    -> Result<(), String> {
        let r = Repo::new()?;
        let first = commit(
            &r,
            "old.rs",
            "before\nlet number = 1;\nafter\n",
            1_700_000_000,
        )?;
        let semantic = commit(
            &r,
            "old.rs",
            "before\nlet number = 2;\nafter\n",
            1_700_000_001,
        )?;
        let path = "-new ü '$(literal).rs";
        r.git(&["mv", "--", "old.rs", path])?;
        let rename = r.commit("line@test.invalid", 1_700_000_002)?;
        let formatting = commit(&r, path, "before\nlet   number=2;\nafter\n", 1_700_000_003)?;
        let head = commit(
            &r,
            path,
            "prefix\nbefore\nlet   number=2;\nafter\n",
            1_700_000_004,
        )?;
        let refs = r.git(&["show-ref"])?;
        let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
        let work = std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?;
        let result = trace(&r.0, &head, path, 3, &|| true)?;
        let ids: BTreeSet<_> = result.cards.iter().map(|c| c.at.commit.as_str()).collect();
        for id in [&first, &semantic, &rename, &formatting] {
            assert!(
                ids.contains(id.as_str()),
                "Missing actual line version {id}"
            );
        }
        assert_eq!(result.start.line, 3);
        assert!(
            result
                .cards
                .iter()
                .any(|c| c.at.commit == rename && c.at.path == path)
        );
        assert!(
            result
                .cards
                .iter()
                .any(|c| c.at.commit == first && c.text == "let number = 1;")
        );
        assert!(
            result
                .cards
                .iter()
                .any(|c| c.at.commit == formatting && c.whitespace_only)
        );
        assert!(
            result
                .cards
                .iter()
                .all(|c| c.attribution.commit == first || c.attribution.commit == semantic)
        );
        assert!(result.stopped.contains("root"));
        assert_eq!(refs, r.git(&["show-ref"])?);
        assert_eq!(
            index,
            std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?
        );
        assert_eq!(
            work,
            std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?
        );
        Ok(())
    }
    #[test]
    fn unequal_replacement_stops_without_guessing_and_merge_parents_remain_explicit()
    -> Result<(), String> {
        let r = Repo::new()?;
        commit(&r, "f", "left\nalpha\nbeta\nright\n", 1_700_000_000)?;
        let head = commit(&r, "f", "left\ncombined\nright\n", 1_700_000_001)?;
        let result = trace(&r.0, &head, "f", 2, &|| true)?;
        assert_eq!(result.cards.len(), 1);
        assert!(result.stopped.contains("ambiguous"));
        r.git(&["branch", "feature"])?;
        commit(&r, "f", "left\nmain line\nright\n", 1_700_000_002)?;
        r.git(&["checkout", "-q", "feature"])?;
        commit(&r, "f", "left\nbranch line\nright\n", 1_700_000_003)?;
        r.git(&["checkout", "-q", "main"])?;
        assert!(
            r.git(&["merge", "--no-ff", "--no-commit", "feature"])
                .is_err()
        );
        r.git(&["rev-parse", "MERGE_HEAD"])?;
        let merge = commit(&r, "f", "left\nresolved line\nright\n", 1_700_000_004)?;
        let result = trace(&r.0, &merge, "f", 2, &|| true)?;
        assert_eq!(result.alternatives.len(), 2);
        assert!(result.stopped.contains("Merge boundary"));
        assert!(
            result
                .alternatives
                .iter()
                .all(|s| s.line == 2 && oid(&s.commit))
        );
        Ok(())
    }
    #[test]
    fn partial_copy_uses_verified_blame_origin_when_whole_file_similarity_is_low()
    -> Result<(), String> {
        let r = Repo::new()?;
        let remembered = "let copied_calculation_with_enough_distinctive_characters = expensive_algorithm(original_input, options);";
        let mut original = (0..100)
            .map(|i| format!("unrelated original source row {i} with distinct filler\n"))
            .collect::<Vec<_>>();
        original[50] = format!("{remembered}\n");
        let source = commit(&r, "source.rs", &original.concat(), 1_700_000_000)?;
        let body = format!("fresh prefix\n{remembered}\nfresh suffix\n");
        let copy = commit(&r, "copy.rs", &body, 1_700_000_001)?;
        let result = trace(&r.0, &copy, "copy.rs", 2, &|| true)?;
        assert!(
            result
                .cards
                .iter()
                .any(|c| c.at.commit == source && c.at.path == "source.rs" && c.at.line == 51)
        );
        assert!(
            result
                .cards
                .iter()
                .any(|c| c.at.commit == copy && c.note.contains("copy/move"))
        );
        Ok(())
    }
    #[test]
    fn parsers_refuse_malformed_ids_quoted_paths_and_ambiguous_duplicate_lines()
    -> Result<(), String> {
        assert!(blame_frame(b"--all 1 1 1\nfilename f\n\tx\n").is_err());
        assert!(quoted_path("\"..\\057escape\"").is_err());
        assert_eq!(quoted_path("\"caf\\303\\251\\tline\"")?, "café\tline");
        assert!(quoted_path("\"truncated\\3\"").is_err());
        assert_eq!(previous_line("x\ny\nx\n", "x\nz\nz\nx\n", 2)?, None);
        assert_eq!(previous_line("red\nred\n", "blue\nblue\n", 1)?, None);
        assert_eq!(
            previous_line("x\nx\n", "x\nx\n", 2)?.map(|(line, _)| line),
            Some(2)
        );
        assert!(!path_ok("../x"));
        assert!(!path_ok(".git/config"));
        assert!(path_ok("-f ü '$(literal)"));
        assert!(
            trace(
                Path::new("/recording/nonexistent"),
                "--all",
                "f",
                1,
                &|| true
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn binary_large_nonregular_and_cancelled_sources_fail_explicitly() -> Result<(), String> {
        let r = Repo::new()?;
        let head = commit(&r, "binary", "a\0b", 1_700_000_000)?;
        assert!(trace(&r.0, &head, "binary", 1, &|| true).is_err());
        assert!(trace(&r.0, &head, "binary", 1, &|| false).is_err());
        let head = commit(&r, "large", &"x".repeat(FILE + 1), 1_700_000_001)?;
        assert!(trace(&r.0, &head, "large", 1, &|| true).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/dev/zero", r.0.join("link")).map_err(|e| e.to_string())?;
            r.git(&["add", "link"])?;
            let head = r.commit("line@test.invalid", 1_700_000_002)?;
            assert!(trace(&r.0, &head, "link", 1, &|| true).is_err());
        }
        Ok(())
    }
}
