//! External recovery evidence, retained through every partial cleanup/retirement failure.
use super::{
    native_owner::Endpoint,
    workspace_authority::{self, WorkspaceAuthority},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use gitcraft_os::{PrivateDirectory, PrivateFileMode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};
const MAX_OWNERS: usize = 4096;
const MAX_PROOF: usize = 9 << 10;
const MAX_MANIFEST: usize = 40 << 20;
const MAX_EVIDENCE: usize = 64 << 20;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    name: String,
    record: String,
    receipt: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    v: u8,
    anchor: String,
    generation: String,
    owner: String,
    entries: Vec<Proof>,
}
struct Entry {
    name: String,
    record: Vec<u8>,
    receipt: Vec<u8>,
    endpoint: Endpoint,
    present: bool,
}
impl Entry {
    fn proof(&self) -> Proof {
        Proof {
            name: self.name.clone(),
            record: B64.encode(&self.record),
            receipt: B64.encode(&self.receipt),
        }
    }
}
fn read(directory: &PrivateDirectory, name: &str, limit: usize) -> Result<Vec<u8>, String> {
    let file = directory
        .open_file(name, PrivateFileMode::Read)
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("oversized ledger entry {name}"));
    }
    Ok(bytes)
}
fn optional(
    directory: &PrivateDirectory,
    name: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, String> {
    match directory.open_file(name, PrivateFileMode::Read) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > limit {
                return Err("oversized ledger entry".into());
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn publish(directory: &PrivateDirectory, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = directory
        .open_file(name, PrivateFileMode::CreateNew)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    gitcraft_os::close_file(file).map_err(|e| e.to_string())
}
fn decode(proof: &Proof, anchor: &str) -> Result<Entry, String> {
    // Reject encoded allocations before decoding untrusted bounded disk evidence.
    if proof.record.len() > 2732 || proof.receipt.len() > 5464 {
        return Err("oversized ledger proof".into());
    }
    let record = B64.decode(&proof.record).map_err(|e| e.to_string())?;
    let receipt = B64.decode(&proof.receipt).map_err(|e| e.to_string())?;
    let endpoint = Endpoint::from_record(&workspace_authority::bound_endpoint(
        &proof.name,
        &record,
        anchor,
    )?)?;
    if !endpoint.confirms(&receipt) {
        return Err("ledger receipt does not authenticate its exact owner".into());
    }
    Ok(Entry {
        name: proof.name.clone(),
        record,
        receipt,
        endpoint,
        present: false,
    })
}
fn merge(entries: &mut BTreeMap<String, Entry>, entry: Entry) -> Result<(), String> {
    if let Some(previous) = entries.get(&entry.name) {
        if previous.record != entry.record
            || (!previous.receipt.is_empty() && previous.receipt != entry.receipt)
        {
            return Err("ledger evidence changed or collided".into());
        }
    } else {
        entries.insert(entry.name.clone(), entry);
    }
    if entries.len() > MAX_OWNERS {
        return Err("workspace recovery exceeds 4096 owners; evidence retained".into());
    }
    Ok(())
}
pub(super) struct NativePlan {
    directory: PrivateDirectory,
    identity: String,
    authority: WorkspaceAuthority,
    owner: String,
    entries: BTreeMap<String, Entry>,
    manifests: Vec<(String, Vec<u8>)>,
}
impl NativePlan {
    /// Caller is the actual durable helper holding the full mutation fence throughout.
    pub(super) fn prepare(
        authority: &WorkspaceAuthority,
        owner: &Path,
        mut active: impl FnMut() -> bool,
    ) -> Result<Self, String> {
        let own = authority.record_name(owner)?.to_owned();
        let directory = PrivateDirectory::open(&authority.root).map_err(|e| e.to_string())?;
        let identity = directory.identity().map_err(|e| e.to_string())?;
        let mut names = BTreeSet::new();
        for entry in std::fs::read_dir(&authority.root).map_err(|e| e.to_string())? {
            let name = entry
                .map_err(|e| e.to_string())?
                .file_name()
                .into_string()
                .map_err(|_| "non-text workspace ledger name")?;
            if !matches!(name.as_str(), "primary" | "fence" | "generation")
                && !name.ends_with(".owner")
                && !name.ends_with(".owner.proof")
                && !name.ends_with(".owner.cleaned")
            {
                return Err(format!("unknown ledger evidence {name}; preserved"));
            }
            names.insert(name);
            if names.len() > MAX_OWNERS * 3 + 3 {
                return Err("workspace ledger inventory limit; evidence retained".into());
            }
        }
        if !names.contains(&own) {
            return Err("cleanup owner is not durably indexed".into());
        }
        let mut entries = BTreeMap::new();
        let mut manifests = Vec::new();
        let mut budget = MAX_EVIDENCE;
        // Validate complete old cleanup manifests before any control request.
        for name in names.iter().filter(|n| n.ends_with(".cleaned")) {
            if !active() {
                return Err("cleanup cancelled or expired; ledger retained".into());
            }
            let bytes = read(&directory, name, MAX_MANIFEST.min(budget))?;
            budget -= bytes.len();
            let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if manifest.v != 1
                || manifest.anchor != authority.anchor
                || format!("{}.cleaned", manifest.owner) != *name
                || !manifest
                    .owner
                    .starts_with(&format!("{}-mutation-", manifest.generation))
                || manifest.entries.len() > MAX_OWNERS
            {
                return Err("invalid cleaned ledger manifest".into());
            }
            let binding = WorkspaceAuthority {
                generation: manifest.generation.clone(),
                mode: super::workspace_authority::FenceMode::Mutation,
                ..authority.clone()
            };
            binding.record_name(&binding.root.join(&manifest.owner))?;
            for proof in manifest.entries {
                merge(&mut entries, decode(&proof, &authority.anchor)?)?;
            }
            manifests.push((name.clone(), bytes));
        }
        for name in names.iter().filter(|n| n.ends_with(".owner") && **n != own) {
            if !active() {
                return Err("cleanup cancelled or expired; ledger retained".into());
            }
            let record = read(&directory, name, 2048.min(budget))?;
            budget -= record.len();
            let endpoint = Endpoint::from_record(&workspace_authority::bound_endpoint(
                name,
                &record,
                &authority.anchor,
            )?)?;
            if let Some(previous) = entries.get_mut(name) {
                if previous.record != record {
                    return Err("owner changed after cleaned proof; evidence retained".into());
                }
                previous.present = true;
            } else {
                entries.insert(
                    name.clone(),
                    Entry {
                        name: name.clone(),
                        record,
                        receipt: Vec::new(),
                        endpoint,
                        present: true,
                    },
                );
            }
        }
        if entries.len() > MAX_OWNERS {
            return Err("workspace recovery exceeds 4096 owners".into());
        }
        for name in names.iter().filter(|n| n.ends_with(".proof")) {
            if !active() {
                return Err("cleanup cancelled or expired; ledger retained".into());
            }
            let bytes = read(&directory, name, MAX_PROOF.min(budget))?;
            budget -= bytes.len();
            let proof: Proof = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if format!("{}.proof", proof.name) != *name {
                return Err("proof filename mismatch".into());
            }
            let entry = decode(&proof, &authority.anchor)?;
            let previous = entries
                .get_mut(&entry.name)
                .ok_or("orphan owner proof without cleaned manifest; recovery unknown")?;
            if previous.record != entry.record
                || (!previous.receipt.is_empty() && previous.receipt != entry.receipt)
            {
                return Err("cached owner proof changed".into());
            }
            previous.receipt = entry.receipt;
        }
        let mut plan = Self {
            directory,
            identity,
            authority: authority.clone(),
            owner: own,
            entries,
            manifests,
        };
        plan.verify()?;
        // All names, records and existing proofs have now been validated.
        for entry in plan.entries.values_mut() {
            if !active() {
                return Err("cleanup cancelled; external evidence retained".into());
            }
            if entry.receipt.is_empty() {
                entry.endpoint.stop()?;
                entry.receipt = entry.endpoint.terminal()?;
                let proof = serde_json::to_vec(&entry.proof()).map_err(|e| e.to_string())?;
                if proof.len() > MAX_PROOF {
                    return Err("oversized authenticated owner proof".into());
                }
                publish(&plan.directory, &format!("{}.proof", entry.name), &proof)?;
            }
        }
        plan.verify()?;
        Ok(plan)
    }
    fn verify(&self) -> Result<(), String> {
        if PrivateDirectory::open(&self.authority.root)
            .and_then(|d| d.identity())
            .map_err(|e| e.to_string())?
            != self.identity
        {
            return Err("external ledger directory replaced".into());
        }
        for entry in self.entries.values() {
            let current = optional(&self.directory, &entry.name, 2048)?;
            if entry.present && current.as_deref() != Some(&entry.record)
                || !entry.present && current.is_some()
            {
                return Err("owner record set changed during recovery".into());
            }
        }
        Ok(())
    }
    pub(super) fn shell_proof(&self) -> Vec<u8> {
        let generations = self.generations();
        let records: Vec<_> = self.entries.values().filter(|e| e.present).collect();
        let mut lines = vec![format!("G {}", generations.len())];
        lines.extend(generations);
        lines.push(format!("R {}", records.len()));
        for entry in records {
            lines.push(entry.name.clone());
            lines.push(entry.record.iter().map(|b| format!("{b:02x}")).collect());
        }
        format!("{}\n", lines.join("\n")).into_bytes()
    }
    fn generations(&self) -> Vec<String> {
        let mut generations = BTreeSet::from([self.authority.generation.clone()]);
        for name in self.entries.keys() {
            generations.insert(name[..64].to_owned());
        }
        generations.into_iter().collect()
    }
    /// Called only after checked CLEAR and all its postconditions succeeded.
    pub(super) fn complete(self) -> Result<Vec<u8>, String> {
        let manifest = Manifest {
            v: 1,
            anchor: self.authority.anchor.clone(),
            generation: self.authority.generation.clone(),
            owner: self.owner.clone(),
            entries: self.entries.values().map(Entry::proof).collect(),
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_MANIFEST {
            return Err("cleaned manifest exceeds limit; external proofs retained".into());
        }
        let manifest_name = format!("{}.cleaned", self.owner);
        finish_cleanup(
            || self.verify(),
            || publish(&self.directory, &manifest_name, &bytes),
            || {
                // Keep every original record/cache until every artifact retirement succeeds.
                for entry in self.entries.values() {
                    entry.endpoint.retire_proven(&entry.receipt)?;
                }
                Ok(())
            },
            || {
                for entry in self.entries.values() {
                    if entry.present {
                        self.directory
                            .remove_checked(&entry.name, &entry.record)
                            .map_err(|e| e.to_string())?;
                    }
                    let name = format!("{}.proof", entry.name);
                    if let Some(proof) = optional(&self.directory, &name, MAX_PROOF)? {
                        let decoded: Proof =
                            serde_json::from_slice(&proof).map_err(|e| e.to_string())?;
                        let cached = decode(&decoded, &self.authority.anchor)?;
                        if cached.name != entry.name
                            || cached.record != entry.record
                            || cached.receipt != entry.receipt
                        {
                            return Err("proof changed before pruning; manifest retained".into());
                        }
                        self.directory
                            .remove_checked(&name, &proof)
                            .map_err(|e| e.to_string())?;
                    }
                }
                for (name, bytes) in &self.manifests {
                    self.directory
                        .remove_checked(name, bytes)
                        .map_err(|e| e.to_string())?;
                }
                self.directory
                    .remove_checked(&manifest_name, &bytes)
                    .map_err(|e| e.to_string())?;
                Ok(())
            },
            || {
                let mut digest = Sha256::new();
                for entry in self.entries.values() {
                    digest.update(entry.name.as_bytes());
                    digest.update([0]);
                    digest.update(&entry.record);
                    digest.update([0]);
                }
                serde_json::to_vec(&serde_json::json!({"v":1,"state":"cleaned","anchor":self.authority.anchor,"generation":self.authority.generation,"records":self.entries.len(),"records_sha256":digest.finalize().iter().map(|b|format!("{b:02x}")).collect::<String>()})).map_err(|e|e.to_string())
            },
        )
    }
}
pub(super) fn cleanup_complete_allowed(code: i32, complete: bool) -> bool {
    code == 0 && complete
}
// Checked completion boundaries used by the actual native ledger and pure recordings.
fn finish_cleanup(
    verify: impl FnOnce() -> Result<(), String>,
    publish: impl FnOnce() -> Result<(), String>,
    retire: impl FnOnce() -> Result<(), String>,
    prune: impl FnOnce() -> Result<(), String>,
    receipt: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    verify()?;
    publish()?;
    retire()?;
    prune()?;
    receipt()
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn every_partial_completion_preserves_failure_and_blocks_later_retirement_or_success() {
        use std::cell::RefCell;
        assert!(!cleanup_complete_allowed(1, true));
        assert!(!cleanup_complete_allowed(0, false));
        assert!(cleanup_complete_allowed(0, true));
        for failure in ["verify", "manifest", "retire", "prune", "receipt", "none"] {
            let events = RefCell::new(Vec::new());
            let stage = |name| {
                events.borrow_mut().push(name);
                if failure == name {
                    Err(format!("{name} failed; ledger retained"))
                } else {
                    Ok(())
                }
            };
            let result = finish_cleanup(
                || stage("verify"),
                || stage("manifest"),
                || stage("retire"),
                || stage("prune"),
                || {
                    stage("receipt")?;
                    Ok(b"cleaned".to_vec())
                },
            );
            assert_eq!(result.is_ok(), failure == "none");
            let expected = ["verify", "manifest", "retire", "prune", "receipt"];
            let end = expected
                .iter()
                .position(|s| *s == failure)
                .map_or(5, |i| i + 1);
            assert_eq!(*events.borrow(), expected[..end]);
            if end < 4 {
                assert!(!events.borrow().contains(&"prune"));
            }
        }
    }
    #[test]
    fn recovery_manifest_merge_never_overwrites_changed_or_unbounded_evidence() {
        let authority = WorkspaceAuthority {
            root: "/synthetic".into(),
            anchor: "unix:1:2".into(),
            generation: "a".repeat(64),
            mode: super::super::workspace_authority::FenceMode::Mutation,
        };
        let name = format!("{}-mutation-{}.owner", authority.generation, "b".repeat(32));
        let endpoint = serde_json::json!({"v":2,"nonce":"c".repeat(64),"path":std::env::temp_dir().join(format!("gitcraft-native-owned-{}","d".repeat(32))),"port":12345,"token":"e".repeat(64)});
        let record =
            workspace_authority::record(&serde_json::to_vec(&endpoint).unwrap(), &authority)
                .unwrap();
        let receipt = serde_json::to_vec(
            &serde_json::json!({"endpoint":endpoint,"state":"stopped","code":0,"complete":true}),
        )
        .unwrap();
        let proof = Proof {
            name: name.clone(),
            record: B64.encode(&record),
            receipt: B64.encode(&receipt),
        };
        let mut entries = BTreeMap::new();
        merge(&mut entries, decode(&proof, &authority.anchor).unwrap()).unwrap();
        let mut wrong = proof.clone();
        wrong.record = B64.encode(b"missing owner");
        assert!(decode(&wrong, &authority.anchor).is_err());
        wrong = proof.clone();
        wrong.receipt = B64.encode(b"missing receipt");
        assert!(decode(&wrong, &authority.anchor).is_err());
        assert!(decode(&proof, "replaced anchor").is_err());
        let mut changed = decode(&proof, &authority.anchor).unwrap();
        changed.record.push(b' ');
        assert!(merge(&mut entries, changed).is_err());
        assert_eq!(entries[&name].record, record);
    }
}
