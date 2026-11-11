//! Dossier history reads, blame parsing, and line attribution state.

use super::*;

pub(super) struct FileVersion {
    pub(super) oid: String,
    pub(super) text: String,
}

pub(super) struct Trace<'a> {
    pub(super) reader: Reader<'a>,
    pub(super) files: HashMap<(String, String), std::sync::Arc<FileVersion>>,
    pub(super) blobs: HashMap<String, std::sync::Arc<FileVersion>>,
    pub(super) bytes: usize,
}

pub(super) fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains(['\0', '\\'])
        && path.split('/').all(|part| {
            !part.is_empty() && !matches!(part, "." | "..") && !part.eq_ignore_ascii_case(".git")
        })
}

pub(super) fn one_line(text: &str, line: usize) -> Result<&str, String> {
    if !(1..=LINES).contains(&line) {
        return Err(crate::i18n::text(
            "backend-line-history-is-limited-to20000-source-lines-04852f",
        )
        .into());
    }
    text.split_terminator('\n').nth(line - 1).ok_or_else(|| {
        crate::i18n::text(
            "backend-the-selected-line-no-longer-exists-at-the-pinned-revision-240588",
        )
        .into()
    })
}

pub(super) fn normal(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub(crate) fn quoted_path(value: &str) -> Result<String, String> {
    if !value.starts_with('"') {
        return if path_ok(value) {
            Ok(value.into())
        } else {
            Err(crate::i18n::text("backend-invalid-git-history-path-660844").into())
        };
    }
    if !value.ends_with('"') || value.len() < 2 {
        return Err(crate::i18n::text("backend-truncated-quoted-git-path-b14143").into());
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
        let b = *bytes
            .get(i)
            .ok_or(crate::i18n::text("backend-truncated-path-escape-c967c4"))?;
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
                let rest = bytes
                    .get(i..i + 2)
                    .ok_or(crate::i18n::text("backend-truncated-octal-path-791865"))?;
                if !rest.iter().all(|b| matches!(b, b'0'..=b'7')) {
                    return Err(crate::i18n::text("backend-invalid-octal-path-cffcb9").into());
                }
                let n = u16::from(b - b'0') * 64
                    + u16::from(rest[0] - b'0') * 8
                    + u16::from(rest[1] - b'0');
                out.push(
                    u8::try_from(n)
                        .map_err(|_| crate::i18n::text("backend-invalid-octal-byte-2cb6f3"))?,
                );
                i += 2;
            }
            _ => {
                return Err(crate::i18n::text("backend-unsupported-git-path-escape-830db8").into());
            }
        }
    }
    let path = String::from_utf8(out)
        .map_err(|_| crate::i18n::text("backend-non-utf-8-history-paths-are-not-traced-996cbd"))?;
    if !path_ok(&path) {
        return Err(crate::i18n::text("backend-unsafe-history-path-efda61").into());
    }
    Ok(path)
}

pub(super) fn blame_frame(out: &[u8]) -> Result<Attribution, String> {
    let text = std::str::from_utf8(out)
        .map_err(|_| crate::i18n::text("backend-blame-returned-non-utf-8-metadata-0fe8ff"))?;
    let mut lines = text.lines();
    let first = lines
        .next()
        .ok_or(crate::i18n::text("backend-empty-line-blame-207bb2"))?;
    if first.len() > 256 {
        return Err(crate::i18n::text("backend-oversized-blame-header-f86d74").into());
    }
    let header: Vec<_> = first.split_whitespace().collect();
    let [commit, line, _, _] = header.as_slice() else {
        return Err(crate::i18n::text("backend-invalid-single-line-blame-header-c0c4d6").into());
    };
    if !oid(commit) {
        return Err(crate::i18n::text("backend-invalid-blame-object-id-643be1").into());
    }
    let line = line
        .parse::<usize>()
        .map_err(|_| crate::i18n::text("backend-invalid-original-line-number-ef5d80"))?;
    if !(1..=LINES).contains(&line) {
        return Err(
            crate::i18n::text("backend-original-line-exceeds-history-limits-b786ac").into(),
        );
    }
    let mut path = None;
    let mut records = 0;
    for line in lines {
        if let Some(value) = line.strip_prefix("filename ") {
            if path.is_some() {
                return Err(crate::i18n::text("backend-duplicate-blame-filename-98efa9").into());
            }
            path = Some(quoted_path(value)?);
        } else if line.starts_with('\t') {
            records += 1;
        }
    }
    if records != 1 {
        return Err(
            crate::i18n::text("backend-blame-did-not-return-exactly-one-line-c10bf8").into(),
        );
    }
    Ok(Attribution {
        commit: (*commit).into(),
        path: path.ok_or(crate::i18n::text("backend-missing-blame-filename-4cccd2"))?,
        line,
    })
}

/// Unique equal text wins; otherwise use two unique context anchors and a small,
/// same-sized replacement block. Repeated/unequal blocks stop, rather than choosing a line.
pub(super) fn previous_line(
    current: &str,
    parent: &str,
    line: usize,
) -> Result<Option<(usize, &'static str)>, String> {
    let now: Vec<_> = current.split_terminator('\n').collect();
    let old: Vec<_> = parent.split_terminator('\n').collect();
    if now.len() > LINES || old.len() > LINES {
        return Err(crate::i18n::text("backend-file-has-over20000-lines-20a678").into());
    }
    let target = normal(
        now.get(line - 1)
            .ok_or(crate::i18n::text("backend-invalid-current-line-1d2e70"))?,
    );
    if current == parent {
        return Ok(Some((
            line,
            crate::i18n::text("backend-identical-raw-file-version-exact-line-position-e1df9d"),
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
            crate::i18n::text("backend-unique-whitespace-normalized-line-match-fe3b3c"),
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
                crate::i18n::text(
                    "backend-parent-line-aligned-by-unique-surrounding-context-inspect-the-ver-a6d5be",
                ),
            )));
        }
    }
    Ok(None)
}

impl<'a> Trace<'a> {
    pub(super) fn file(
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
        let record = records
            .next()
            .ok_or(crate::i18n::text("backend-missing-tree-record-147457"))?;
        if records.next().is_some() {
            return Err(
                crate::i18n::text("backend-line-history-expected-one-exact-file-83fede").into(),
            );
        }
        let split = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(crate::i18n::text("backend-invalid-tree-record-bf9356"))?;
        if &record[split + 1..] != path.as_bytes() {
            return Err(crate::i18n::text(
                "backend-tree-path-did-not-match-its-literal-request-ea4301",
            )
            .into());
        }
        let fields: Vec<_> = std::str::from_utf8(&record[..split])
            .map_err(|_| crate::i18n::text("backend-invalid-tree-metadata-1d2e46"))?
            .split_whitespace()
            .collect();
        let [mode, kind, id] = fields.as_slice() else {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        };
        if !matches!(*mode, "100644" | "100755") || *kind != "blob" || !oid(id) {
            return Err(crate::i18n::text(
                "backend-line-history-only-reads-regular-text-blobs-links-submodules-are-n-8668cf",
            )
            .into());
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
            return Err(crate::i18n::text(
                "backend-line-history-reached-its2mib-file-or32mib-snapshot-budget-06c25d",
            )
            .into());
        }
        let bytes = self.reader.run(&["cat-file", "blob", id], vec![], &[0])?;
        if bytes.len() != size || bytes.contains(&0) {
            return Err(crate::i18n::text(
                "backend-binary-or-incomplete-file-version-cannot-be-traced-af9e9a",
            )
            .into());
        }
        let actual = self.reader.run(
            &["hash-object", "--no-filters", "--stdin"],
            bytes.clone(),
            &[0],
        )?;
        if actual.trim_ascii() != id.as_bytes() {
            return Err(crate::i18n::text(
                "backend-blob-identity-changed-while-tracing-history-2ff14d",
            )
            .into());
        }
        let text = String::from_utf8(bytes).map_err(|_| {
            crate::i18n::text("backend-non-utf-8-file-versions-cannot-be-traced-d40721")
        })?;
        if text.split_terminator('\n').count() > LINES {
            return Err(crate::i18n::text("backend-file-has-over20000-lines-20a678").into());
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
    pub(super) fn parents(
        &self,
        commit: &str,
    ) -> Result<(Vec<String>, String, String, i64, String), String> {
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
            return Err(crate::i18n::text("backend-version-metadata-exceeds16kib-daf692").into());
        }
        let fields: Vec<_> = out.split(|b| *b == 0).collect();
        let [parents, author, email, time, subject, tail] = fields.as_slice() else {
            return Err(crate::i18n::text("backend-invalid-version-metadata-4fe146").into());
        };
        if !tail.iter().all(u8::is_ascii_whitespace) {
            return Err(crate::i18n::text("backend-trailing-version-metadata-1d44b4").into());
        }
        let text = |b: &[u8]| {
            String::from_utf8(b.to_vec()).map_err(|_| {
                crate::i18n::text("backend-invalid-utf-8-version-metadata-b59dcf").to_owned()
            })
        };
        let parents: Vec<_> = text(parents)?
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if parents.len() > 8 || parents.iter().any(|id| !oid(id)) {
            return Err(crate::i18n::text(
                "backend-unsupported-or-malformed-commit-parent-list-337df5",
            )
            .into());
        }
        Ok((
            parents,
            text(author)?,
            text(email)?,
            text(time)?
                .parse()
                .map_err(|_| crate::i18n::text("backend-invalid-author-time-982ba5"))?,
            text(subject)?,
        ))
    }
    pub(super) fn old_path(
        &self,
        parent: &str,
        current: &str,
        path: &str,
    ) -> Result<Option<String>, String> {
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
                return Err(crate::i18n::text(
                    "backend-rename-detection-exceeded10000-changed-paths-4d972b",
                )
                .into());
            }
            let first = fields
                .next()
                .ok_or(crate::i18n::text("backend-truncated-rename-record-dd5b2b"))?;
            let rename = matches!(status.first(), Some(b'R' | b'C'));
            let second = if rename {
                Some(fields.next().ok_or(crate::i18n::text(
                    "backend-truncated-rename-destination-f609c2",
                ))?)
            } else {
                None
            };
            if second.is_some_and(|new| new == path.as_bytes()) {
                let old = std::str::from_utf8(first).map_err(|_| {
                    crate::i18n::text("backend-renamed-history-path-is-not-utf-8-30395e")
                })?;
                if !path_ok(old) {
                    return Err(
                        crate::i18n::text("backend-unsafe-renamed-history-path-7b2f2d").into(),
                    );
                }
                return Ok(Some(old.into()));
            }
            if !rename && first == path.as_bytes() && status.first() == Some(&b'A') {
                return Ok(None);
            }
        }
        Ok(Some(path.into()))
    }
    pub(super) fn touch(&self, parent: &str, path: &str) -> Result<Option<String>, String> {
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
            .map_err(|_| crate::i18n::text("backend-invalid-file-history-921c83"))?
            .trim();
        if value.is_empty() {
            Ok(None)
        } else if oid(value) {
            Ok(Some(value.into()))
        } else {
            Err(crate::i18n::text("backend-invalid-file-history-object-id-8473d2").into())
        }
    }
}
