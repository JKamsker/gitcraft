//! Conflict decisions kept in git: each one a blob (JSON) under
//! `refs/gitcraft/conflict-decisions/<key>`, so that they stay with the repository, are never
//! pushed unless asked, and can travel with the palette's sync. Blocking: from the
//! background.

use std::collections::HashMap;
use std::path::Path;

use super::record::Decision;
use crate::git;

/// Where the decisions are kept.
pub const REFS: &str = "refs/gitcraft/conflict-decisions/";

/// Where a remote's decisions are fetched to, before the newer of each pair is kept.
const FETCHED: &str = "refs/gitcraft/fetched-decisions/";

/// The largest decision read: a bigger blob (fetched from elsewhere) is left out.
const MAX_BLOB: usize = 1 << 20;

/// Whether `key` can name a decision (what [`super::record::key`] makes).
fn valid_key(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The decisions stored under `names` (`<prefix><key>` each), by key, in one call. Those
/// stored in a form this version can't read are left out.
fn read(repo: &Path, prefix: &str, keys: &[&String]) -> Result<HashMap<String, Decision>, String> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let input: String = keys.iter().map(|k| format!("{prefix}{k}\n")).collect();
    let out = git::run_captured(repo, &["cat-file", "--batch"], Some(input.as_bytes()))?;
    if out.code != Some(0) {
        return Err(crate::i18n::message(
            "repo-core-0500",
            &[("trim", format!("{}", out.stderr.trim()).into())],
        ));
    }
    Ok(parse_batch(&out.raw, keys))
}

/// The decisions stored for `keys`, by key, in one call.
pub fn load(repo: &Path, keys: &[String]) -> Result<HashMap<String, Decision>, String> {
    let wanted: Vec<&String> = keys.iter().filter(|k| valid_key(k)).collect();
    read(repo, REFS, &wanted)
}

/// `cat-file --batch` output for the refs of `keys`, in order: `<oid> <type> <size>\n<data>\n`,
/// or `<ref> missing\n`. Sizes count bytes: read as bytes, each blob decoded on its own, so
/// that one that isn't text (or a tree) leaves the others as they are.
fn parse_batch(out: &[u8], keys: &[&String]) -> HashMap<String, Decision> {
    let mut found = HashMap::new();
    let mut rest = out;
    for key in keys {
        let Some(end) = rest.iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        rest = &rest[end + 1..];
        let mut words = header.split(' ');
        let (_, kind, size) = (words.next(), words.next(), words.next());
        let Some(size) = size.and_then(|s| s.parse::<usize>().ok()) else {
            // `missing` (or `ambiguous`): nothing stored for it.
            continue;
        };
        let Some(data) = rest.get(..size) else {
            break;
        };
        rest = rest.get(size..).unwrap_or_default();
        rest = rest.strip_prefix(b"\n").unwrap_or(rest);
        if kind == Some("blob")
            && size <= MAX_BLOB
            && let Ok(text) = std::str::from_utf8(data)
            && let Ok(decision) = serde_json::from_str::<Decision>(text)
        {
            found.insert((*key).clone(), decision);
        }
    }
    found
}

/// Stores `decision` under `key`, over what was stored for it.
pub fn save(repo: &Path, key: &str, decision: &Decision) -> Result<(), String> {
    if !valid_key(key) {
        return Err(crate::i18n::message(
            "repo-core-0502",
            &[("key", format!("{}", key).into())],
        ));
    }
    let json = serde_json::to_string_pretty(decision)
        .map_err(|e| crate::i18n::message("repo-core-0503", &[("e", format!("{}", e).into())]))?;
    let written = git::run_captured(
        repo,
        &["hash-object", "-w", "--stdin"],
        Some(json.as_bytes()),
    )?;
    let blob = written.stdout.trim();
    if written.code != Some(0) || blob.is_empty() {
        return Err(crate::i18n::message(
            "repo-core-0507",
            &[("trim", format!("{}", written.stderr.trim()).into())],
        ));
    }
    git::run(Some(repo), &["update-ref", &format!("{REFS}{key}"), blob])
        .map(|_| ())
        .map_err(|e| crate::i18n::message("repo-core-0510", &[("e", format!("{}", e).into())]))
}

/// The keys of the decisions under `prefix`.
fn keys_under(repo: &Path, prefix: &str) -> Result<Vec<String>, String> {
    let out = git::run(Some(repo), &["for-each-ref", "--format=%(refname)", prefix])?;
    Ok(out
        .lines()
        .filter_map(|r| r.strip_prefix(prefix))
        .filter(|k| valid_key(k))
        .map(str::to_owned)
        .collect())
}

/// Brings `remote`'s decisions here: fetched aside first, then, for each conflict, the newer
/// of the two kept (a decision made here since isn't lost). With `send`, then sends them all
/// to it, over its own (which were just taken in). Says what was done.
pub fn sync(repo: &Path, remote: &str, send: bool) -> Result<String, String> {
    let aside = format!("{FETCHED}{remote}/");
    let refspec = format!("+{REFS}*:{aside}*");
    git::run(Some(repo), &["fetch", "--no-tags", remote, &refspec]).map_err(|e| {
        crate::i18n::message(
            "repo-core-0515",
            &[
                ("remote", format!("{}", remote).into()),
                ("e", format!("{}", e).into()),
            ],
        )
    })?;
    let theirs_keys = keys_under(repo, &aside)?;
    let theirs_refs: Vec<&String> = theirs_keys.iter().collect();
    let theirs = read(repo, &aside, &theirs_refs)?;
    let ours = read(repo, REFS, &theirs_refs)?;
    let mut taken = 0;
    for (key, decision) in &theirs {
        if ours.get(key).is_none_or(|mine| decision.at > mine.at) {
            git::run(
                Some(repo),
                &[
                    "update-ref",
                    &format!("{REFS}{key}"),
                    &format!("{aside}{key}"),
                ],
            )?;
            taken += 1;
        }
    }
    for key in &theirs_keys {
        let _ = git::run(Some(repo), &["update-ref", "-d", &format!("{aside}{key}")]);
    }
    let got = match taken {
        0 => crate::i18n::message(
            "repo-core-0517",
            &[("remote", format!("{}", remote).into())],
        ),
        1 => crate::i18n::message(
            "repo-core-0518",
            &[("remote", format!("{}", remote).into())],
        ),
        n => crate::i18n::message(
            "repo-core-0519",
            &[("n", (n).into()), ("remote", format!("{}", remote).into())],
        ),
    };
    if !send {
        return Ok(got);
    }
    let mine = keys_under(repo, REFS)?;
    if mine.is_empty() {
        return Ok(crate::i18n::message(
            "repo-core-0520",
            &[("got", format!("{}", got).into())],
        ));
    }
    git::run(Some(repo), &["push", remote, &format!("+{REFS}*:{REFS}*")]).map_err(|e| {
        crate::i18n::message(
            "repo-core-0522",
            &[
                ("got", format!("{}", got).into()),
                ("remote", format!("{}", remote).into()),
                ("e", format!("{}", e).into()),
            ],
        )
    })?;
    Ok(crate::i18n::message(
        "repo-core-0523",
        &[
            ("got", format!("{}", got).into()),
            ("count", (mine.len()).into()),
            (
                "s",
                format!("{}", if mine.len() == 1 { "" } else { "s" }).into(),
            ),
            ("remote", format!("{}", remote).into()),
        ],
    ))
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
        let (a, b, c, d) = (
            "a".repeat(32),
            "b".repeat(32),
            "c".repeat(32),
            "d".repeat(32),
        );
        let json = r#"{"path":"p","resolution":"r\n","ours":"o","theirs":"t"}"#;
        let mut out = format!("{REFS}{a} missing\n").into_bytes();
        // Not text: its size in bytes still tells where the next one starts.
        out.extend(b"0001 blob 4\n\xff\xfe\xfd\xfc\n");
        out.extend(format!("0123 blob {}\n{json}\n", json.len()).into_bytes());
        out.extend(b"0456 tree 3\n\x00\x01\x02\n");
        let found = parse_batch(&out, &[&a, &b, &c, &d]);
        assert_eq!(found.len(), 1);
        assert_eq!(found.get(&c).map(|d| d.resolution.as_str()), Some("r\n"));
        assert!(parse_batch(b"", &[&a]).is_empty());
    }
}
