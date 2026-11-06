//! Conflict decisions kept in git: each one a blob (JSON) under
//! `refs/gitcraft/conflict-decisions/<key>`, so that they stay with the repository, are never
//! pushed unless asked, and can travel with a push or fetch of those refs. Blocking: from the
//! background.

use std::collections::HashMap;
use std::path::Path;

use super::record::Decision;
use crate::git;
use crate::repo::ops;

/// Where the decisions are kept.
pub const REFS: &str = "refs/gitcraft/conflict-decisions/";

/// The refspec that moves every decision, replacing what the other side has for the same
/// conflict (they are blobs: there is no fast-forward between two of them).
const REFSPEC: &str = "+refs/gitcraft/conflict-decisions/*:refs/gitcraft/conflict-decisions/*";

/// Whether `key` can name a decision (what [`super::record::key`] makes).
fn valid_key(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The decisions stored for `keys`, by key, in one call. Those stored in a form this version
/// can't read are left out.
pub fn load(repo: &Path, keys: &[String]) -> Result<HashMap<String, Decision>, String> {
    let wanted: Vec<&String> = keys.iter().filter(|k| valid_key(k)).collect();
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }
    let input: String = wanted.iter().map(|k| format!("{REFS}{k}\n")).collect();
    let out = git::run_captured(repo, &["cat-file", "--batch"], Some(input.as_bytes()))?;
    if out.code != Some(0) {
        return Err(format!(
            "Could not read the conflict decisions: {}",
            out.stderr.trim()
        ));
    }
    Ok(parse_batch(&out.stdout, &wanted))
}

/// `cat-file --batch` output for the refs of `keys`, in order: `<oid> blob <size>\n<data>\n`,
/// or `<ref> missing\n`.
fn parse_batch(out: &str, keys: &[&String]) -> HashMap<String, Decision> {
    let mut found = HashMap::new();
    let mut rest = out;
    for key in keys {
        let Some((header, after)) = rest.split_once('\n') else {
            break;
        };
        let mut words = header.split(' ');
        let (_, kind, size) = (words.next(), words.next(), words.next());
        let Some(size) = size.and_then(|s| s.parse::<usize>().ok()) else {
            // `missing` (or `ambiguous`): nothing stored for it.
            rest = after;
            continue;
        };
        let Some(data) = after.get(..size) else {
            break;
        };
        rest = after
            .get(size..)
            .map_or("", |r| r.strip_prefix('\n').unwrap_or(r));
        if kind == Some("blob")
            && let Ok(decision) = serde_json::from_str::<Decision>(data)
        {
            found.insert((*key).clone(), decision);
        }
    }
    found
}

/// Stores `decision` under `key`, over what was stored for it.
pub fn save(repo: &Path, key: &str, decision: &Decision) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("{key} can't name a conflict decision"));
    }
    let json = serde_json::to_string_pretty(decision)
        .map_err(|e| format!("Could not write the decision down: {e}"))?;
    let written = git::run_captured(
        repo,
        &["hash-object", "-w", "--stdin"],
        Some(json.as_bytes()),
    )?;
    let blob = written.stdout.trim();
    if written.code != Some(0) || blob.is_empty() {
        return Err(format!(
            "Could not store the decision: {}",
            written.stderr.trim()
        ));
    }
    git::run(Some(repo), &["update-ref", &format!("{REFS}{key}"), blob])
        .map(|_| ())
        .map_err(|e| format!("Could not store the decision: {e}"))
}

/// The git steps that send every decision to `remote`.
pub fn push_ops(remote: &str) -> Vec<Vec<String>> {
    ops(&[&["push", remote, REFSPEC]])
}

/// The git steps that bring `remote`'s decisions here.
pub fn fetch_ops(remote: &str) -> Vec<Vec<String>> {
    ops(&[&["fetch", remote, REFSPEC]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_keys_name_refs() {
        assert!(valid_key(&super::super::record::key("a", "b")));
        assert!(!valid_key("../../heads/main"));
        assert!(!valid_key(""));
        assert!(!valid_key(&"g".repeat(32)));
    }

    #[test]
    fn batch_output_with_missing_and_odd_entries() {
        let a = "a".repeat(32);
        let b = "b".repeat(32);
        let c = "c".repeat(32);
        let json = r#"{"path":"p","resolution":"r\n","ours":"o","theirs":"t"}"#;
        let out = format!(
            "{REFS}{a} missing\n0123 blob {}\n{json}\n0456 blob 3\nnot\n",
            json.len()
        );
        let found = parse_batch(&out, &[&a, &b, &c]);
        assert_eq!(found.len(), 1);
        assert_eq!(found.get(&b).map(|d| d.resolution.as_str()), Some("r\n"));
        assert!(parse_batch("", &[&a]).is_empty());
    }
}
