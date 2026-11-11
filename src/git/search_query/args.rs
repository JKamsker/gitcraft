//! Turning a query into the `git log` options that search for it.

use super::parse::{QueryError, SearchQuery, parse};

/// How a query is searched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// No operators: messages, authors and hashes (or, in Code, `-S`), as always.
    Plain { query: String, code: bool },
    /// `git log` once per pass, with these filter options each, limited to `paths`; the
    /// commits any pass finds.
    Operators {
        passes: Vec<Vec<String>>,
        paths: Vec<String>,
        /// Free text that may be (the start of) a commit's hash, as in a plain search.
        hash: Option<HashLookup>,
    },
}

/// A commit named by the free text, found when the operators' filters match it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashLookup {
    pub prefix: String,
    pub filters: Vec<String>,
}

/// How to search `query` (trimmed), with free text looked for in the code (`-S`) when
/// `code`, else in messages and authors; or why it can't be searched.
pub fn prepare(query: &str, code: bool) -> Result<Plan, QueryError> {
    let Some(parsed) = parse(query)? else {
        return Ok(Plan::Plain {
            query: query.to_owned(),
            code,
        });
    };
    let hash = (!code && looks_like_hash(&parsed.text)).then(|| HashLookup {
        prefix: parsed.text.clone(),
        filters: filters(&parsed, ignores_case(&parsed, code)),
    });
    Ok(Plan::Operators {
        passes: passes(&parsed, code)?,
        paths: parsed.paths.clone(),
        hash,
    })
}

/// Four hex digits or more, as the plain search takes a hash.
fn looks_like_hash(text: &str) -> bool {
    text.len() >= 4 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether text is matched in any letter case: when names or messages are matched (git has
/// one switch, which then applies to `-S` / `-G` too). Changes alone (`contents:`,
/// `regex:`, Code) match the case typed, as the plain Code search does.
fn ignores_case(q: &SearchQuery, code: bool) -> bool {
    !q.authors.is_empty()
        || !q.committers.is_empty()
        || !q.messages.is_empty()
        || (!code && !q.text.is_empty())
}

/// The filter options of each `git log` run. Free text matches the message or the author,
/// which git can't say in one run (its `--author` and `--grep` must both match): one run
/// for each, unless `author:` already names the author. In Code, free text is `-S`.
fn passes(q: &SearchQuery, code: bool) -> Result<Vec<Vec<String>>, QueryError> {
    let mut common = filters(q, ignores_case(q, code));
    if q.text.is_empty() {
        return Ok(vec![common]);
    }
    if code {
        if q.contents.is_some() || q.regex.is_some() {
            return Err(QueryError {
                message: crate::i18n::text("backend-in-code-the-free-text-is-already-looked-for-in-the-changes-drop-c-597d4f")
                    .to_owned(),
                at: 0,
            });
        }
        common.push(format!("-S{}", q.text));
        return Ok(vec![common]);
    }
    let mut by_message = common.clone();
    by_message.push(format!("--grep={}", q.text));
    let mut out = vec![by_message];
    if q.authors.is_empty() {
        common.push(format!("--author={}", q.text));
        out.push(common);
    }
    Ok(out)
}

/// The options for everything but the free text and the paths. Text matches as typed (no
/// regex), in any letter case when `ignore_case`; several `message:` must all match,
/// several `author:` or `committer:` any of them.
pub(super) fn filters(q: &SearchQuery, ignore_case: bool) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if ignore_case {
        args.push("-i".to_owned());
    }
    args.extend(["--fixed-strings", "--all-match", "--use-mailmap"].map(str::to_owned));
    args.extend(q.authors.iter().map(|a| format!("--author={a}")));
    args.extend(q.committers.iter().map(|c| format!("--committer={c}")));
    args.extend(q.messages.iter().map(|m| format!("--grep={m}")));
    if let Some(after) = &q.after {
        args.push(format!("--since={after}"));
    }
    if let Some(before) = &q.before {
        args.push(format!("--until={before}"));
    }
    if let Some(text) = &q.contents {
        args.push(format!("-S{text}"));
    }
    if let Some(regex) = &q.regex {
        args.push(format!("-G{regex}"));
    }
    match q.merges {
        Some(true) => args.push("--merges".to_owned()),
        Some(false) => args.push("--no-merges".to_owned()),
        None => {}
    }
    args
}
