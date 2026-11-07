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
    fn release_range_is_pinned_mailmapped_and_read_only_across_a_merge() -> Result<(), String> {
        let repo = super::super::tests::Repo::new()?;
        let base = repo.commit("base@test.invalid", 1_700_000_000)?;
        std::fs::write(
            repo.0.join(".mailmap"),
            "Canonical Name <canonical@test.invalid> View tester <alias@test.invalid>\n",
        )
        .map_err(|e| e.to_string())?;
        repo.git(&["branch", "feature"])?;
        let main = repo.commit("alias@test.invalid", 1_700_000_100)?;
        repo.git(&["checkout", "-q", "feature"])?;
        let feature = repo.commit("alias@test.invalid", 1_700_000_200)?;
        repo.git(&["checkout", "-q", "main"])?;
        repo.git(&["merge", "--no-ff", "-qm", "Release merge", "feature"])?;
        let tip = repo.git(&["rev-parse", "HEAD"])?;
        repo.git(&["tag", "release", &tip])?;
        let refs = repo.git(&["show-ref"])?;
        let index = std::fs::read(repo.0.join(".git/index")).map_err(|e| e.to_string())?;
        let commits = range(&repo.0, &base, &tip, &|| true)?;
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].hash, tip);
        assert!(
            commits
                .iter()
                .filter(|c| c.hash == main || c.hash == feature)
                .all(|c| c.author == "Canonical Name")
        );
        assert_eq!(repo.git(&["show-ref"])?, refs);
        assert_eq!(
            std::fs::read(repo.0.join(".git/index")).map_err(|e| e.to_string())?,
            index
        );
        repo.git(&["tag", "-f", "release", &base])?;
        assert_eq!(
            range(&repo.0, &base, &tip, &|| true)?
                .iter()
                .map(|c| &c.hash)
                .collect::<Vec<_>>(),
            commits.iter().map(|c| &c.hash).collect::<Vec<_>>()
        );
        assert!(range(&repo.0, &base, &tip, &|| false).is_err());
        assert!(range(&repo.0, "", "--all", &|| true).is_err());
        Ok(())
    }
    #[test]
    fn postcard_reads_practice_before_a_broken_active_profile() -> Result<(), String> {
        let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
        let result = (|| {
            let tip = crate::git::practice_run(&repo, &["rev-parse", "HEAD"])?;
            let profile = crate::connections::Context {
                error: Some("Profile unavailable".into()),
                ..Default::default()
            };
            let commits = profile.run(|| range(&repo, "", tip.trim(), &|| true))?;
            assert!(!commits.is_empty());
            Ok(())
        })();
        if let Some(root) = repo.parent() {
            let _ = std::fs::remove_dir_all(root);
        }
        result
    }
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
