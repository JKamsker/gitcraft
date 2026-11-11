//! Rulebook responsibilities for echo.
use super::*;

pub(super) fn location() -> Result<PathBuf, String> {
    crate::portable::data_dir()
        .map(|p| p.join("reviewer-echo.json"))
        .ok_or("No personal data directory".into())
}
pub(super) fn load() -> Result<Book, String> {
    let p = location()?;
    match std::fs::read(p) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub(super) fn save(book: &Book) -> Result<(), String> {
    let _lock = WRITING.lock().map_err(|_| "Rulebook writer unavailable")?;
    let p = location()?;
    let parent = p.parent().ok_or("No rulebook folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, p).map_err(|e| e.to_string())
}
pub(super) fn own_comments(d: &PrDetails) -> Result<Vec<Comment>, String> {
    let viewer = d
        .extras
        .viewer
        .as_deref()
        .ok_or("The forge did not identify your signed-in account")?;
    Ok(d.extras
        .threads
        .iter()
        .flat_map(|t| &t.comments)
        .filter(|c| !c.pending && c.author.eq_ignore_ascii_case(viewer) && !c.url.is_empty())
        .map(|c| Comment {
            url: c.url.clone(),
            body: c.body.clone(),
            author: c.author.clone(),
        })
        .collect())
}
pub(super) fn validate_rule(rule: &Rule, comments: &[Comment]) -> Result<(), String> {
    if !(rule.source.starts_with("https://") || rule.source.starts_with("http://"))
        || !comments.iter().any(|c| c.url == rule.source)
    {
        return Err("Rule has no imported comment citation".into());
    }
    if rule.pattern.is_empty() || rule.pattern.len() > 4000 {
        return Err("Rule pattern is empty or too large".into());
    }
    regex::Regex::new(&rule.pattern)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
pub(super) fn check_patch(patch: &str, book: &Book) -> Result<Vec<Hit>, String> {
    if !book.enabled {
        return Ok(vec![]);
    }
    let rules = book
        .rules
        .iter()
        .filter(|r| r.enabled)
        .map(|r| {
            validate_rule(r, &book.comments)?;
            Ok((r, regex::Regex::new(&r.pattern).map_err(|e| e.to_string())?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut path = String::new();
    let mut line = 0;
    let mut hits = vec![];
    let mut in_hunk = false;
    for text in patch.lines() {
        if text.starts_with("diff --git ") {
            in_hunk = false;
            path.clear();
            continue;
        }
        if !in_hunk && let Some(p) = text.strip_prefix("+++ ") {
            let decoded = git::unquote_path(p);
            path = decoded.strip_prefix("b/").unwrap_or(&decoded).to_owned();
            continue;
        }
        if text.starts_with("@@ ") {
            in_hunk = true;
            line = text
                .split_whitespace()
                .nth(2)
                .and_then(|n| n.trim_start_matches('+').split(',').next()?.parse().ok())
                .unwrap_or(0);
            continue;
        }
        if !in_hunk {
            continue;
        }
        if let Some(added) = text.strip_prefix('+') {
            for (rule, re) in &rules {
                if re.is_match(added) {
                    hits.push(Hit {
                        path: path.clone(),
                        line,
                        text: added.to_owned(),
                        summary: rule.summary.clone(),
                        url: rule.source.clone(),
                    });
                }
            }
            line += 1;
        } else if text.starts_with(' ') {
            line += 1;
        }
    }
    Ok(hits)
}
