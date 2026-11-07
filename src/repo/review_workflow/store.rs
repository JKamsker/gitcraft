use crate::git;
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

/// Separate namespaces and a digest of the complete key avoid ref injection and collisions
/// from lossy sanitization. Objects and refs work for SSH repositories too.
pub(super) fn reference(namespace: &str, key: &str) -> String {
    format!(
        "refs/gitcraft/workflow/{:x}/{:x}",
        md5::compute(namespace),
        md5::compute(key)
    )
}

pub(super) fn load<T: DeserializeOwned>(
    repo: &Path,
    namespace: &str,
    key: &str,
) -> Result<Option<T>, String> {
    let name = reference(namespace, key);
    let oid = git::run_ok_codes(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", &name],
        &[0, 1],
    )?;
    if oid.trim().is_empty() {
        return Ok(None);
    }
    let text = git::run(Some(repo), &["cat-file", "blob", oid.trim()])?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("Invalid review evidence: {e}"))
}

pub(super) fn save<T: Serialize>(
    repo: &Path,
    namespace: &str,
    key: &str,
    value: &T,
) -> Result<(), String> {
    let name = reference(namespace, key);
    let old = git::run_ok_codes(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", &name],
        &[0, 1],
    )?;
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let oid = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &text)?;
    // Detect a ref changed while this individual save was writing its blob.
    git::run(Some(repo), &["update-ref", &name, oid.trim(), old.trim()]).map(|_| ())
}
