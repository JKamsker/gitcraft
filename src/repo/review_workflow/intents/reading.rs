//! Reading responsibilities for intents.
use super::*;

pub(super) const CACHE_ROOT: &str = "refs/gitcraft/intent-remotes/";
pub(super) const CARD_LIMIT: usize = 8192;
pub(super) const IMPORT_LIMIT: usize = 4 * 1024 * 1024;
#[derive(Default)]
pub(super) struct Reading {
    pub(super) rows: Vec<Seen>,
    pub(super) quarantine: Vec<String>,
}
pub(super) fn source_roots(source: Option<&str>) -> Result<Vec<String>, String> {
    let mut roots = vec![ROOT.to_owned()];
    if let Some(source) = source {
        if source.is_empty()
            || source.len() > 64
            || !source
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("Invalid intent cache identity".into());
        }
        roots.push(format!("{CACHE_ROOT}{source}/"));
    }
    Ok(roots)
}
pub(super) fn validate(card: &Card, reference: &str, root: &str, at: u64) -> Result<(), String> {
    let hex = |s: &str| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let relative = reference
        .strip_prefix(root)
        .ok_or("Intent is outside the selected source")?;
    let (author, id) = relative
        .split_once('/')
        .ok_or("Invalid intent ref identity")?;
    if !hex(author)
        || !hex(id)
        || id != card.id
        || author != format!("{:x}", md5::compute(&card.email))
    {
        return Err("Intent identity does not match its source ref".into());
    }
    let short = |text: &str, max: usize| text.len() <= max && !text.chars().any(|c| c.is_control());
    if card.path.is_empty()
        || !short(&card.path, 512)
        || card.path.starts_with('/')
        || card.path.contains(['\\', ':'])
        || card
            .path
            .trim_end_matches('/')
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | "..") || p.eq_ignore_ascii_case(".git"))
        || card.author.trim().is_empty()
        || !short(&card.author, 128)
        || card.email.trim().is_empty()
        || !short(&card.email, 254)
        || !short(&card.symbol, 256)
        || card.message.trim().is_empty()
        || !short(&card.message, 1024)
    {
        return Err("Intent fields exceed the short-note bounds or contain an unsafe path/control character".into());
    }
    if card.created > at.saturating_add(300)
        || card.expires < card.created
        || card.expires > card.created.saturating_add(86400)
    {
        return Err("Intent expiry must be within 24 hours of its creation".into());
    }
    Ok(())
}
pub(super) fn collect_selected(
    source: Option<&str>,
    at: u64,
    mut list: impl FnMut(&str) -> Result<Vec<(String, String)>, String>,
    mut blob: impl FnMut(&str, usize) -> Result<String, String>,
) -> Result<Reading, String> {
    let mut reading = Reading::default();
    let mut cards = BTreeMap::<(String, String), Seen>::new();
    let mut bytes: usize = 0;
    let mut rejected = 0;
    for root in source_roots(source)? {
        // List only this source. Foreign cache content is never parsed or deduplicated.
        let refs = list(&root)?;
        if refs.len() > 5000 {
            return Err("More than 5000 refs in the selected intent source; archive old notes before loading".into());
        }
        for (reference, object) in refs {
            if !reference.starts_with(&root) {
                continue;
            }
            // Reaching the exact budget is complete only if no relevant refs remain,
            // including in the next selected source. Refuse before reading another blob
            // so a partial collection cannot be installed or marked freshly synced.
            if bytes >= IMPORT_LIMIT {
                return Err("Selected intent metadata exceeds the 4MiB import budget; read refused because more notes remain".into());
            }
            let result: Result<Seen, String> = (|| {
                let text = blob(&object, CARD_LIMIT.min(IMPORT_LIMIT - bytes))?;
                bytes = bytes.saturating_add(text.len());
                if text.len() > CARD_LIMIT || bytes > IMPORT_LIMIT {
                    return Err("Intent exceeds the metadata import budget".into());
                }
                let card: Card =
                    serde_json::from_str(&text).map_err(|e| format!("Invalid intent JSON: {e}"))?;
                validate(&card, &reference, &root, at)?;
                Ok(Seen {
                    reference: reference.clone(),
                    card,
                    local: root == ROOT,
                })
            })();
            match result {
                Ok(seen) => {
                    cards.insert((root.clone(), seen.reference.clone()), seen);
                }
                Err(error) => {
                    rejected += 1;
                    if reading.quarantine.len() < 32 {
                        reading.quarantine.push(format!("{reference}: {error}"));
                    }
                }
            }
        }
    }
    if rejected > reading.quarantine.len() {
        reading.quarantine.push(format!(
            "{} additional invalid records omitted",
            rejected - reading.quarantine.len()
        ));
    }
    reading.rows = cards.into_values().collect();
    Ok(reading)
}
pub(super) fn read_selected(repo: &Path, source: Option<&str>) -> Result<Reading, String> {
    collect_selected(
        source,
        now(),
        |root| {
            let listing = git::run(
                Some(repo),
                &["for-each-ref", "--format=%(objectname) %(refname)", root],
            )?;
            listing
                .lines()
                .map(|line| {
                    let (object, reference) =
                        line.split_once(' ').ok_or("Malformed intent ref listing")?;
                    Ok((reference.to_owned(), object.to_owned()))
                })
                .collect()
        },
        |object, budget| {
            let path = format!("{object}:intent.json");
            let size = git::run(Some(repo), &["cat-file", "-s", &path])?
                .trim()
                .parse::<usize>()
                .map_err(|_| "Invalid intent blob size")?;
            if size > budget {
                return Err(format!("Intent blob exceeds the {budget}-byte budget"));
            }
            git::run(Some(repo), &["cat-file", "blob", &path])
        },
    )
}
pub(super) fn read(repo: &Path) -> Result<Vec<Seen>, String> {
    Ok(read_selected(repo, None)?.rows)
}
#[cfg(test)]
pub(super) fn read_active(repo: &Path, source: Option<&str>) -> Result<Vec<Seen>, String> {
    Ok(read_selected(repo, source)?.rows)
}
