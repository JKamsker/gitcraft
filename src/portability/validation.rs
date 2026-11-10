//! Validate repository metadata, path leaves, refs, and object identities.
use super::*;

impl Repository {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let r = self;
        config::validate(&r.config)?;
        leaf(&r.name)?;
        text(&r.previous_path, 4096)?;
        if r.profile_ids.len() > 64 {
            return Err("Too many profile associations".into());
        }
        if r.remotes.len() > 32
            || r.worktrees.len() > 64
            || r.hidden.len() > 2000
            || r.notices.len() > 64
        {
            return Err("Repository metadata exceeds limits".into());
        }
        let mut remote_names = std::collections::BTreeSet::new();
        for remote in &r.remotes {
            reference(&remote.name)?;
            if !remote_names.insert(&remote.name) {
                return Err("Duplicate remote name".into());
            }
            if sanitize_url(&remote.fetch)?.0 != remote.fetch {
                return Err("Remote URL contains credentials or nonportable decorations".into());
            }
            if remote.push.len() > 16 {
                return Err("Too many push URLs".into());
            }
            for u in &remote.push {
                if sanitize_url(u)?.0 != *u {
                    return Err("Push URL contains credentials".into());
                }
            }
        }
        for s in [&r.author_name, &r.author_email].into_iter().flatten() {
            text(s, 512)?;
        }
        for s in &r.hidden {
            full_ref(s)?;
        }
        if let Some(b) = &r.branch {
            reference(b)?;
        }
        if let Some(c) = &r.commit {
            oid(c)?;
        }
        let mut worktrees = std::collections::BTreeSet::new();
        for w in &r.worktrees {
            leaf(&w.name)?;
            if !worktrees.insert(&w.name) {
                return Err("Duplicate worktree name".into());
            }
            text(&w.previous_path, 4096)?;
            if let Some(b) = &w.branch {
                reference(b)?;
            }
            oid(&w.commit)?;
        }
        for n in &r.notices {
            text(n, 1024)?;
        }
        Ok(())
    }
}

pub(super) fn text(s: &str, max: usize) -> Result<(), String> {
    if s.len() > max || s.chars().any(char::is_control) {
        Err("Text contains control characters or exceeds its limit".into())
    } else {
        Ok(())
    }
}

pub(super) fn windows_reserved(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or_default().to_ascii_lowercase();
    ["con", "prn", "aux", "nul"].contains(&stem.as_str())
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
        })
}

pub(crate) fn leaf(s: &str) -> Result<(), String> {
    text(s, 100)?;
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.ends_with(['.', ' '])
        || s.starts_with('.')
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || windows_reserved(s)
    {
        return Err("Choose a plain, unique folder name".into());
    }
    Ok(())
}

pub(crate) fn reference(s: &str) -> Result<(), String> {
    text(s, 256)?;
    if s.is_empty()
        || s.starts_with(['-', '.', '/'])
        || s.ends_with(['.', '/'])
        || s.contains("..")
        || s.contains("//")
        || s.contains("@{")
        || s.ends_with(".lock")
        || s.split('/').any(|p| p.starts_with('.'))
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
    {
        return Err("Invalid portable ref or remote name".into());
    }
    Ok(())
}

pub(super) fn full_ref(s: &str) -> Result<(), String> {
    if !s.starts_with("refs/heads/") && !s.starts_with("refs/remotes/") {
        return Err("Only branch visibility refs are portable".into());
    }
    reference(s)
}

pub(super) fn host(s: &str) -> Result<(), String> {
    text(s, 256)?;
    if s.is_empty()
        || s.starts_with(['-', '.'])
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
    {
        return Err("Invalid expected server hostname".into());
    }
    Ok(())
}

pub(super) fn oid(s: &str) -> Result<(), String> {
    if !matches!(s.len(), 40 | 64) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid stored commit ID".into());
    }
    Ok(())
}
