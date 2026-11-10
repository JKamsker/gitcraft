//! Recovery commit/tree decoding and bounded text excerpts.

use super::*;

pub(super) fn read_commits(
    reader: &Reader<'_>,
    ids: &BTreeSet<String>,
    report: &mut Report,
) -> Result<BTreeMap<String, Commit>, String> {
    let ids: Vec<_> = ids.iter().cloned().collect();
    let mut requests = Vec::new();
    for item in batch::check(reader, &ids)? {
        match item.info {
            Some((kind, size)) if kind == "commit" && size <= 256 * 1024 => {
                requests.push((item.oid, size))
            }
            None => report.missing += 1,
            _ => {
                report.limited = true;
            }
        }
    }
    let mut commits = BTreeMap::new();
    for group in batch::groups(&requests) {
        for (oid, bytes) in batch::read(reader, group, "commit")? {
            commits.insert(oid.clone(), parse_commit(&oid, &bytes)?);
        }
    }
    Ok(commits)
}

pub(super) fn parse_commit(oid: &str, bytes: &[u8]) -> Result<Commit, String> {
    let split = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or(crate::i18n::text("backend-malformed-commit-object-20d1ec"))?;
    let mut tree = None;
    let mut parents = Vec::new();
    let mut when = None;
    for line in bytes[..split].split(|b| *b == b'\n') {
        if let Some(value) = line.strip_prefix(b"tree ") {
            let value = std::str::from_utf8(value)
                .map_err(|_| crate::i18n::text("backend-invalid-commit-tree-f82ac0"))?;
            if tree.is_some() || !valid_oid(value) {
                return Err(crate::i18n::text("backend-invalid-commit-tree-f82ac0").into());
            }
            tree = Some(value.to_owned());
        }
        if let Some(value) = line.strip_prefix(b"parent ") {
            let value = std::str::from_utf8(value)
                .map_err(|_| crate::i18n::text("backend-invalid-commit-parent-87c5f3"))?;
            if !valid_oid(value) || parents.len() >= 64 {
                return Err(crate::i18n::text("backend-invalid-commit-parents-8430cf").into());
            }
            parents.push(value.to_owned());
        }
        if let Some(value) = line.strip_prefix(b"committer ") {
            when = value
                .rsplit(|b| b.is_ascii_whitespace())
                .filter(|part| !part.is_empty())
                .nth(1)
                .and_then(|time| std::str::from_utf8(time).ok())
                .and_then(|time| time.parse::<i64>().ok());
        }
    }
    let message = bytes
        .get(split + 2..)
        .ok_or(crate::i18n::text("backend-missing-commit-message-832b0d"))?;
    let subject = message.split(|b| *b == b'\n').next().unwrap_or_default();
    Ok(Commit {
        oid: oid.into(),
        tree: tree.ok_or(crate::i18n::text("backend-missing-commit-tree-2e21e3"))?,
        parents,
        when: when.ok_or(crate::i18n::text("backend-missing-commit-time-3db252"))?,
        subject: String::from_utf8_lossy(subject)
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
    })
}

pub(super) fn stash_shape(commit: &Commit, parents: &BTreeMap<String, Commit>) -> bool {
    matches!(commit.parents.len(), 2 | 3)
        && (commit.subject.starts_with("WIP on ") || commit.subject.starts_with("On "))
        && parents.get(&commit.parents[1]).is_some_and(|index| {
            index.parents == commit.parents[..1] && index.subject.starts_with("index on ")
        })
        && (commit.parents.len() == 2
            || parents.get(&commit.parents[2]).is_some_and(|untracked| {
                untracked.parents.is_empty() && untracked.subject.starts_with("untracked files on ")
            }))
}

pub(super) fn tree_entries(
    reader: &Reader<'_>,
    tree: &str,
    report: &mut Report,
) -> Result<Vec<(String, String, String)>, String> {
    let bytes = reader.run(&["ls-tree", "-r", "-z", "--full-tree", tree], None)?;
    let mut entries = Vec::new();
    for row in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let Some(tab) = row.iter().position(|b| *b == b'\t') else {
            return Err(crate::i18n::text("backend-invalid-tree-record-bf9356").into());
        };
        let info = std::str::from_utf8(&row[..tab])
            .map_err(|_| crate::i18n::text("backend-invalid-tree-metadata-1d2e46"))?;
        let parts: Vec<_> = info.split_whitespace().collect();
        let [mode, kind, oid] = parts.as_slice() else {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        };
        if !valid_oid(oid) {
            return Err(crate::i18n::text("backend-invalid-tree-object-id-58c8bf").into());
        }
        if *kind != "blob" {
            continue;
        }
        let Ok(path) = std::str::from_utf8(&row[tab + 1..]) else {
            report.limited = true;
            continue;
        };
        if path.len() > 4096 || path.chars().any(char::is_control) {
            report.limited = true;
            continue;
        }
        if !matches!(*mode, "100644" | "100755" | "120000") {
            return Err(crate::i18n::text("backend-invalid-blob-mode-f621df").into());
        }
        if entries.len() >= ENTRIES {
            return Err(crate::i18n::text(
                "backend-a-tree-exceeds-15000-file-entries-narrow-recovery-with-git-in-the-447484",
            )
            .into());
        }
        entries.push((path.into(), (*mode).into(), (*oid).into()));
    }
    Ok(entries)
}

pub(super) fn excerpt(text: &str, at: usize) -> (usize, String) {
    let line = text[..at].bytes().filter(|b| *b == b'\n').count() + 1;
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(120)
        .map_or(0, |(i, _)| i);
    let mut excerpt: String = text[start..].chars().take(400).collect();
    if start > 0 {
        excerpt.insert(0, '…');
    }
    if text[start..].chars().count() > 400 {
        excerpt.push('…');
    }
    (line, excerpt)
}

pub(in crate::git) fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
