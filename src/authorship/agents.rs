//! The coding agents a commit names itself: as its author (an agent's own account) or in a
//! `Co-authored-by:` trailer. Only what the commit says, and only agents' own identities: a
//! person called Claude, Haider or Geminiani is a person.

/// Agents' own addresses, exactly.
const EMAILS: [(&str, &str); 8] = [
    ("noreply@anthropic.com", "Claude"),
    ("cursoragent@cursor.com", "Cursor"),
    ("noreply@openai.com", "Codex"),
    ("codex@openai.com", "Codex"),
    ("noreply@aider.chat", "aider"),
    ("amp@ampcode.com", "Amp"),
    ("openhands@all-hands.dev", "OpenHands"),
    ("noreply@google.com", "Gemini"),
];

/// Agents' names, as a whole name given with a no-reply or bot address (GitHub's
/// `…+Copilot@users.noreply.github.com`, `devin-ai-integration[bot]`).
const NAMES: [(&str, &str); 12] = [
    ("claude", "Claude"),
    ("cursor agent", "Cursor"),
    ("copilot", "Copilot"),
    ("codex", "Codex"),
    ("chatgpt", "Codex"),
    ("aider", "aider"),
    ("devin-ai-integration", "Devin"),
    ("google-labs-jules", "Jules"),
    ("jules", "Jules"),
    ("gemini", "Gemini"),
    ("openhands", "OpenHands"),
    ("windsurf", "Windsurf"),
];

/// The agent `name` with `email` is, if that is an agent's own identity.
pub fn identity(name: &str, email: &str) -> Option<&'static str> {
    let email = email.trim().to_lowercase();
    if let Some((_, agent)) = EMAILS.iter().find(|(e, _)| *e == email) {
        return Some(agent);
    }
    let name = name.trim().to_lowercase();
    let automated = email.contains("noreply") || email.contains("[bot]") || name.ends_with("[bot]");
    if !automated {
        return None;
    }
    NAMES
        .iter()
        .find(|(n, _)| {
            // The name itself (`Copilot`, `copilot-swe-agent[bot]`), never a person's name that
            // only starts so (`Claude Smith`).
            name == *n
                || name
                    .strip_suffix("[bot]")
                    .is_some_and(|b| b == *n || b.starts_with(&format!("{n}-")))
                || email.contains(&format!("+{n}@"))
                || email.contains(&format!("+{n}["))
        })
        .map(|(_, agent)| *agent)
}

/// `Name <email>` taken apart.
fn split(value: &str) -> (&str, &str) {
    match value.split_once('<') {
        Some((name, rest)) => (name.trim(), rest.split('>').next().unwrap_or_default()),
        None => (value.trim(), ""),
    }
}

/// The agents a commit's message names in `Co-authored-by:` trailers: in its last paragraph
/// (the trailer block), not quoted in its body.
pub fn co_authors(message: &str) -> Vec<&'static str> {
    let trimmed = message.trim_end();
    let block = trimmed.rsplit("\n\n").next().unwrap_or(trimmed);
    // The subject alone is no trailer block.
    if block == trimmed
        && !trimmed.contains('\n')
        && !trimmed.to_lowercase().starts_with("co-authored-by")
    {
        return Vec::new();
    }
    let mut out: Vec<&'static str> = block
        .lines()
        .filter(|l| !l.starts_with([' ', '\t']))
        .filter_map(|l| {
            let (key, value) = l.split_once(':')?;
            if !key.trim().eq_ignore_ascii_case("co-authored-by") {
                return None;
            }
            let (name, email) = split(value);
            identity(name, email)
        })
        .collect();
    out.dedup();
    out
}

/// The agent a commit's author is, when the author is an agent's own account.
pub fn author_agent(name: &str, email: &str) -> Option<&'static str> {
    identity(name, email)
}
