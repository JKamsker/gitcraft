//! Anchors responsibilities for storyboard.
use super::*;

pub(super) fn key(d: &PrDetails) -> String {
    format!("{}#{}", d.url, d.number)
}
pub(super) fn oid(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map(|s| s.trim().into())
}
pub(super) fn file_blob(repo: &Path, commit: &str, path: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}:{path}"),
        ],
    )
    .map(|s| s.trim().into())
}
pub(super) fn card(
    repo: &Path,
    d: &PrDetails,
    f: &PrFiles,
    form: CardForm,
) -> Result<Card, String> {
    if form.text.trim().is_empty() {
        return Err("Give the card its explanation".into());
    }
    let commit = oid(
        repo,
        if form.commit.trim().is_empty() {
            &f.to
        } else {
            form.commit.trim()
        },
    )?;
    let anchor = if form.test {
        let tree = git::run(Some(repo), &["rev-parse", &format!("{commit}^{{tree}}")])?
            .trim()
            .to_owned();
        let verdict = git::read_results(repo, git::DEFAULT_TEST)?
            .get(&tree)
            .copied()
            .ok_or("No test-lane result for this tree; run Test commits first")?;
        let command =
            git::test_command(repo, git::DEFAULT_TEST)?.ok_or("No test command recorded")?;
        Anchor::Test {
            commit,
            tree,
            command: format!(
                "Configured when pinned (execution command not recorded by test lane): {command}"
            ),
            verdict: verdict.word().into(),
        }
    } else {
        let path = form.path.trim().to_owned();
        if path.is_empty() {
            return Err("Choose a repository-relative path".into());
        }
        let blob = file_blob(repo, &commit, &path)?;
        let line = if form.line.trim().is_empty() {
            0
        } else {
            form.line
                .parse()
                .map_err(|_| "Line must be a positive number")?
        };
        if line > 0 {
            let text = git::run(Some(repo), &["cat-file", "blob", &blob])?;
            if line > text.lines().count() as u64 {
                return Err("Line is outside this revision's file".into());
            }
        }
        Anchor::Code {
            commit,
            path,
            line,
            blob,
        }
    };
    Ok(Card {
        phase: ["Old failure", "Invariant", "Fix", "Result"]
            .get(form.phase)
            .unwrap_or(&"Fix")
            .to_string(),
        text: form.text,
        base: f.from.clone(),
        head: d.head_oid.clone(),
        anchor,
    })
}
pub(super) fn show_card(repo: &Path, card: Card, f: &PrFiles) -> Result<Shown, String> {
    let mut shown = Shown {
        card: card.clone(),
        target: None,
        stale: None,
    };
    match &card.anchor {
        Anchor::Test { tree, .. } => {
            let now = git::run(Some(repo), &["rev-parse", &format!("{}^{{tree}}", f.to)])?;
            if now.trim() != tree {
                shown.stale = Some("Current tree differs from the pinned test result".into());
            }
        }
        Anchor::Code {
            commit,
            path,
            line,
            blob,
        } => {
            let mut target = commit.clone();
            if card.head != f.to {
                let pairs = git::range_diff(
                    repo,
                    &git::Series::new(&card.base, &card.head),
                    &git::Series::new(&f.from, &f.to),
                )?;
                if commit == &card.head {
                    target = f.to.clone();
                } else if commit == &card.base {
                    target = f.from.clone();
                } else if let Some(new) = pairs
                    .pairs
                    .iter()
                    .find(|p| p.old.as_ref().is_some_and(|c| &c.hash == commit))
                    .and_then(|p| p.new.as_ref())
                {
                    target = new.hash.clone();
                } else {
                    shown.stale =
                        Some("The pinned commit has no replacement in this series".into());
                }
            }
            let renames = git::diff_files(repo, commit, Some(&target))?;
            let path = renames
                .iter()
                .find(|f| f.old_path.as_deref() == Some(path))
                .map_or(path.as_str(), |f| f.path.as_str())
                .to_owned();
            match file_blob(repo, &target, &path) {
                Ok(now) if &now == blob => shown.target = Some((target, path, *line)),
                Ok(_) => {
                    shown.stale = Some(
                        "The card's file changed; open the pinned original to review it".into(),
                    )
                }
                Err(_) => {
                    shown.stale = Some("The card's file was removed or is unavailable".into())
                }
            }
        }
    }
    Ok(shown)
}
pub(super) fn read(repo: &Path, d: &PrDetails, f: &PrFiles) -> Result<Vec<Shown>, String> {
    store::load::<Board>(repo, "storyboard", &key(d))?
        .unwrap_or_default()
        .cards
        .into_iter()
        .map(|c| show_card(repo, c, f))
        .collect()
}
