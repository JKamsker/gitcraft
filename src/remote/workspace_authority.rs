//! Immutable authority carried to the actual durable owner; client-side validation alone
//! does not grant dispatch. This module is staged while Workspace integration is pending.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FenceMode {
    Dispatch,
    Mutation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceAuthority {
    pub root: PathBuf,
    pub anchor: String,
    pub generation: String,
    pub mode: FenceMode,
}
impl WorkspaceAuthority {
    pub fn native_grant(&self) -> gitcraft_os::WorkspaceGrant {
        gitcraft_os::WorkspaceGrant {
            root: self.root.clone(),
            anchor: self.anchor.clone(),
            generation: self.generation.clone(),
        }
    }
    pub fn record_name<'a>(&self, path: &'a std::path::Path) -> Result<&'a str, String> {
        if path.parent() != Some(self.root.as_path()) {
            return Err("workspace owner record is outside its ledger".into());
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid workspace owner record name")?;
        validate_record_name(name, &self.generation, self.mode)?;
        Ok(name)
    }
}
fn validate_record_name(name: &str, generation: &str, mode: FenceMode) -> Result<(), String> {
    let prefix = format!(
        "{}-{}-",
        generation,
        match mode {
            FenceMode::Dispatch => "dispatch",
            FenceMode::Mutation => "mutation",
        }
    );
    let nonce = name
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix(".owner"))
        .ok_or("owner record generation/kind mismatch")?;
    if !hex(nonce, 32) || !hex(generation, 64) {
        return Err("invalid workspace owner nonce/generation".into());
    }
    Ok(())
}

fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    v: u8,
    anchor: String,
    generation: String,
    kind: FenceMode,
    endpoint: serde_json::Value,
}
pub(crate) fn record(endpoint: &[u8], authority: &WorkspaceAuthority) -> Result<Vec<u8>, String> {
    if !hex(&authority.generation, 64)
        || authority.anchor.is_empty()
        || authority.anchor.len() > 128
    {
        return Err("invalid workspace owner binding".into());
    }
    let envelope = Envelope {
        v: 3,
        anchor: authority.anchor.clone(),
        generation: authority.generation.clone(),
        kind: authority.mode,
        endpoint: serde_json::from_slice(endpoint).map_err(|e| e.to_string())?,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    if bytes.len() > 2048 {
        return Err("workspace owner record exceeds 2048 bytes".into());
    }
    Ok(bytes)
}
pub(crate) fn bound_endpoint(name: &str, bytes: &[u8], anchor: &str) -> Result<Vec<u8>, String> {
    if bytes.len() > 2048 {
        return Err("oversized workspace owner".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.v != 3 || envelope.anchor != anchor {
        return Err("workspace owner anchor/version mismatch".into());
    }
    validate_record_name(name, &envelope.generation, envelope.kind)?;
    serde_json::to_vec(&envelope.endpoint).map_err(|e| e.to_string())
}
/// Preserve legacy raw v1/v2 records; v3 binding remains in the durable original bytes.
/// The endpoint's own strict parser/authentication still determines stop authority.
pub(crate) fn endpoint(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > 2048 {
        return Err("oversized owner record".into());
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value.get("v").and_then(|v| v.as_u64()) != Some(3) {
        return Ok(bytes.to_vec());
    }
    let envelope: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if envelope.v != 3
        || !hex(&envelope.generation, 64)
        || envelope.anchor.is_empty()
        || envelope.anchor.len() > 128
    {
        return Err("invalid workspace owner binding".into());
    }
    serde_json::to_vec(&envelope.endpoint).map_err(|e| e.to_string())
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn records_bind_generation_kind_and_ledger_before_publication()
    -> Result<(), Box<dyn std::error::Error>> {
        let authority = WorkspaceAuthority {
            root: "/synthetic/ledger".into(),
            anchor: "unix:1:2".into(),
            generation: "a".repeat(64),
            mode: FenceMode::Dispatch,
        };
        let name = format!("{}-dispatch-{}.owner", authority.generation, "b".repeat(32));
        assert_eq!(authority.record_name(&authority.root.join(&name))?, name);
        for wrong in [
            PathBuf::from("/foreign").join(&name),
            authority.root.join(name.replace("dispatch", "mutation")),
            authority.root.join("old.owner"),
        ] {
            assert!(authority.record_name(&wrong).is_err());
        }
        let raw = serde_json::to_vec(&serde_json::json!({"v":2,"nonce":"c".repeat(64),
            "path":std::env::temp_dir().join(format!("gitcraft-native-owned-{}","d".repeat(32))),
            "port":12345,"token":"e".repeat(64)}))?;
        let bound = record(&raw, &authority)?;
        assert!(super::super::native_owner::Endpoint::from_record(&endpoint(&bound)?).is_ok());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&endpoint(&bound)?)?,
            serde_json::from_slice::<serde_json::Value>(&raw)?
        );
        assert_eq!(endpoint(&raw)?, raw);
        let mut malformed: serde_json::Value = serde_json::from_slice(&bound)?;
        malformed["generation"] = "PID 424242".into();
        assert!(endpoint(&serde_json::to_vec(&malformed)?).is_err());
        Ok(())
    }
}
