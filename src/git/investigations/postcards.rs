//! Immutable release ranges for composed postcards. Read only, mailmap aware, never fetches objects.
use super::{Reader, oid};
use crate::git::Commit;
use std::path::Path;
pub(crate) const LIMIT: usize = 200;
pub(crate) fn range(
    repo: &Path,
    base: &str,
    target: &str,
    active: &dyn Fn() -> bool,
) -> Result<Vec<Commit>, String> {
    if !oid(target) || (!base.is_empty() && !oid(base)) {
        return Err("Choose immutable commit IDs for both range ends".into());
    }
    let reader = Reader::new(repo, active);
    let mut input = format!("{target}\n");
    if !base.is_empty() {
        input.push_str(&format!("^{base}\n"));
    }
    let output = reader.run(
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--topo-order",
            "--max-count=201",
            "--date=short",
            "--format=%H%x00%P%x00%aN%x00%ad%x00%s%x00",
            "--stdin",
        ],
        input.into_bytes(),
        &[0],
    )?;
    parse(&output)
}
fn parse(output: &[u8]) -> Result<Vec<Commit>, String> {
    let text = std::str::from_utf8(output).map_err(|_| "Release history is not UTF-8")?;
    let fields: Vec<_> = text.split('\0').collect();
    let (records, tail) = fields.as_chunks::<5>();
    if !tail.iter().all(|f| f.trim().is_empty()) {
        return Err("Release history returned incomplete metadata".into());
    }
    if records.len() > LIMIT {
        return Err(
            "This range exceeds the postcard's 200-commit limit; choose a narrower release range"
                .into(),
        );
    }
    let commits: Result<Vec<_>, String> = records
        .iter()
        .map(|[hash, parents, author, date, summary]| {
            let hash = hash.trim();
            if !oid(hash) || !parents.split_whitespace().all(oid) {
                return Err("Release history returned invalid commit IDs".into());
            }
            Ok(Commit {
                hash: hash.into(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                author: (*author).into(),
                date: (*date).into(),
                summary: (*summary).into(),
                ..Default::default()
            })
        })
        .collect();
    let commits = commits?;
    if commits.is_empty() {
        return Err("The selected release range contains no commits".into());
    }
    Ok(commits)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_metadata_keeps_subject_boundaries_and_rejects_truncation() -> Result<(), String> {
        let h = "a".repeat(40);
        let p = "b".repeat(40);
        let data =
            format!("{h}\0{p}\0Canonical Name\02026-11-07\0Subject with unit separator \u{1f}\0\n");
        let commits = parse(data.as_bytes())?;
        assert_eq!(commits[0].author, "Canonical Name");
        assert!(commits[0].summary.ends_with('\u{1f}'));
        assert!(parse(data.trim_end_matches(['\n', '\0']).as_bytes()).is_err());
        assert!(parse(data.repeat(LIMIT + 1).as_bytes()).is_err());
        Ok(())
    }
}
