use super::*;
#[derive(Default)]
struct Recording {
    pub(super) calls: Vec<(Store, Vec<String>, Vec<u8>)>,
    pub(super) fail: Option<String>,
    pub(super) changed: bool,
    pub(super) published: bool,
    pub(super) marker: Option<String>,
    pub(super) preexisting: bool,
    pub(super) stage: Option<&'static str>,
    pub(super) events: Vec<&'static str>,
}
impl Backend for Recording {
    fn keep_message(&mut self) -> Result<String, String> {
        Ok(format!("gitcraft-bundle-keep-v1-{}", "b".repeat(32)))
    }
    fn retire_keep(&mut self, keep: &Keep) -> Result<(), String> {
        assert!(self.published && self.events.contains(&"terminal"));
        self.events.push("retire");
        assert_eq!(keep.hash, "c".repeat(40));
        if self.stage == Some("changed-marker") {
            self.marker = Some("another-owner".into());
        }
        if self.stage == Some("retire") || self.marker.as_deref() != Some(&keep.message) {
            return Err("marker ownership changed or cleanup unconfirmed; retained".into());
        }
        self.marker = None;
        Ok(())
    }
    fn unchanged(&mut self) -> Result<(), String> {
        if self.changed || (self.marker.is_some() && self.stage == Some("identity")) {
            Err("destination moved".into())
        } else {
            Ok(())
        }
    }
    fn git(&mut self, store: Store, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>, String> {
        self.calls.push((
            store,
            args.iter().map(|s| (*s).into()).collect(),
            input.clone(),
        ));
        if self.fail.as_deref() == args.first().copied() {
            return Err("owned cleanup unconfirmed".into());
        }
        if store == Store::Destination && args[0] == "index-pack" {
            let message = self.keep_message()?;
            assert!(args.contains(&format!("--keep={message}").as_str()));
            if self.preexisting {
                assert!(self.marker.is_some());
            } else {
                assert!(self.marker.is_none());
                self.marker = Some(message);
            }
            self.events.push("protect");
            if self.stage == Some("index") {
                return Err("index terminal unconfirmed".into());
            }
            if self.stage == Some("receipt") {
                return Ok(b"keep\tmalformed\n".to_vec());
            }
            return Ok(format!(
                "{}\t{}\n",
                if self.preexisting { "pack" } else { "keep" },
                "c".repeat(40)
            )
            .into_bytes());
        }
        if store == Store::Destination && args[0] == "cat-file" {
            // Simulate independent repack attempts during separate object checks:
            // this pack is still protected regardless of the Workspace fence.
            assert!(self.marker.is_some());
            self.events.push("check/repack");
            if self.stage == Some("objects") {
                return Err("object check terminal unconfirmed".into());
            }
        }
        Ok(match args[0] {
            "rev-parse" => b"sha1\n".to_vec(),
            "cat-file" => String::from_utf8(input)
                .map_err(|error| error.to_string())?
                .lines()
                .map(|l| format!("{} commit 10\n", l.trim_end_matches("^{commit}")))
                .collect::<String>()
                .into_bytes(),
            "update-ref" => {
                assert!(self.marker.is_some());
                self.events.extend(["prepare/repack", "commit/repack"]);
                self.published = true;
                if self.stage == Some("publication") {
                    return Err("publication terminal unconfirmed".into());
                }
                self.events.push("terminal");
                vec![]
            }
            _ => vec![],
        })
    }
}
fn refs() -> Vec<BundleRef> {
    vec![BundleRef {
        name: "refs/heads/main".into(),
        oid: "a".repeat(40),
    }]
}
#[test]
fn verification_never_publishes_and_import_uses_identical_pack_then_atomic_create()
-> Result<(), String> {
    let refs = refs();
    let mapped = vec![BundleRef {
        name: "refs/bundle-import/test/heads/main".into(),
        oid: refs[0].oid.clone(),
    }];
    let mut read = Recording::default();
    pipeline(&mut read, b"PACK-recording", &refs, &[], &mapped, false)?;
    assert!(!read.published && read.calls.iter().all(|(_, a, _)| a[0] != "update-ref"));
    let mut write = Recording::default();
    pipeline(&mut write, b"PACK-recording", &refs, &[], &mapped, true)?;
    let packs: Vec<_> = write
        .calls
        .iter()
        .filter(|(_, a, _)| a[0] == "index-pack")
        .collect();
    assert_eq!(packs.len(), 2);
    assert_eq!(packs[0].2, packs[1].2);
    let (_, args, input) = write.calls.last().ok_or("missing recorded publication")?;
    assert_eq!(args, &["update-ref", "--no-deref", "--stdin"]);
    assert_eq!(
        String::from_utf8(input.clone()).map_err(|error| error.to_string())?,
        format!(
            "start\ncreate {} {}\nprepare\ncommit\n",
            mapped[0].name, mapped[0].oid
        )
    );
    assert!(write.published);
    Ok(())
}
#[test]
fn failures_stale_targets_and_cleanup_uncertainty_cannot_become_success() {
    for fail in ["init", "cat-file", "index-pack", "fsck", "update-ref"] {
        let mut backend = Recording {
            fail: Some(fail.into()),
            ..Default::default()
        };
        assert!(
            pipeline(&mut backend, b"PACK", &refs(), &refs(), &refs(), true)
                .is_err_and(|error| error.contains("unconfirmed"))
        );
        assert!(!backend.published);
    }
    let mut moved = Recording {
        changed: true,
        ..Default::default()
    };
    assert!(pipeline(&mut moved, b"PACK", &refs(), &[], &refs(), true).is_err());
    assert!(
        !moved
            .calls
            .iter()
            .any(|(s, a, _)| *s == Store::Destination && a[0] == "index-pack")
    );
    assert!(
        completed(
            Err("cancelled; owned cleanup unconfirmed".into()),
            Err("ledger retained".into()),
            true
        )
        .is_err_and(
            |error| error.contains("unconfirmed; quarantine cleanup failed: ledger retained")
        )
    );
    assert!(
        completed(Ok(()), Err("ref cleanup failed".into()), true)
            .is_err_and(|error| error.starts_with("Ref publication completed"))
    );
}
#[test]
fn every_git_command_has_no_lazy_no_network_no_hooks_and_fixed_memory_policy() -> Result<(), String>
{
    let args = policy("/private/quarantine.git", "/private/no-hooks");
    assert_eq!(args[0], "--no-lazy-fetch");
    for flag in [
        "protocol.allow=never",
        "pack.threads=1",
        "core.hooksPath=/private/no-hooks",
        "credential.helper=",
    ] {
        assert!(args.iter().any(|a| a == flag));
    }
    let words = wrapper(true, "\"/objects\"", &args)?;
    assert!(words.iter().any(|s| s.contains("resource.setrlimit")));
    assert!(words.iter().all(|s| !s.contains("${@:3}")));
    Ok(())
}
#[test]
fn owned_keep_spans_object_checks_ref_prepare_commit_and_terminal_before_retirement()
-> Result<(), String> {
    let mut backend = Recording::default();
    pipeline(&mut backend, b"PACK", &refs(), &refs(), &refs(), true)?;
    assert_eq!(
        backend.events,
        [
            "protect",
            "check/repack",
            "check/repack",
            "prepare/repack",
            "commit/repack",
            "terminal",
            "retire"
        ]
    );
    assert!(backend.marker.is_none() && backend.published);
    Ok(())
}
#[test]
fn indexing_receipt_object_identity_and_publication_uncertainty_retain_keep() {
    for stage in ["index", "receipt", "objects", "identity", "publication"] {
        let mut backend = Recording {
            stage: Some(stage),
            ..Default::default()
        };
        assert!(pipeline(&mut backend, b"PACK", &refs(), &refs(), &refs(), true).is_err());
        assert!(
            backend
                .marker
                .as_deref()
                .is_some_and(|marker| marker.starts_with("gitcraft-bundle-keep-v1-")),
            "{stage}"
        );
        assert!(!backend.events.contains(&"retire"), "{stage}");
        assert_eq!(backend.published, stage == "publication");
    }
}
#[test]
fn preexisting_keep_refuses_publication_and_is_never_retired_or_overwritten() {
    for marker in [
        "preexisting-owner".into(),
        format!("gitcraft-bundle-keep-v1-{}", "b".repeat(32)),
    ] {
        let mut backend = Recording {
            preexisting: true,
            marker: Some(marker.clone()),
            ..Default::default()
        };
        assert!(
            pipeline(&mut backend, b"PACK", &refs(), &[], &refs(), true)
                .is_err_and(|error| error.contains("another keep marker"))
        );
        // Even coincidentally matching contents do not turn another creator's
        // `pack` receipt into our `keep` ownership receipt.
        assert_eq!(backend.marker.as_ref(), Some(&marker));
        assert_eq!(backend.events, ["protect"]);
        assert!(!backend.published);
    }
}
#[test]
fn changed_marker_or_retirement_failure_preserves_marker_and_confirmed_publication() {
    for stage in ["changed-marker", "retire"] {
        let mut backend = Recording {
            stage: Some(stage),
            ..Default::default()
        };
        let error = pipeline(&mut backend, b"PACK", &refs(), &[], &refs(), true);
        assert!(error.is_err_and(|error| error.starts_with("Ref publication completed")));
        assert!(backend.published && backend.marker.is_some());
        assert_eq!(backend.events.last(), Some(&"retire"));
    }
}
#[test]
fn pack_keep_receipt_requires_one_exact_frame_and_matching_hash_width() {
    let message = format!("gitcraft-bundle-keep-v1-{}", "b".repeat(32));
    for width in [40, 64] {
        assert!(
            created_keep(
                format!("keep\t{}\n", "c".repeat(width)).as_bytes(),
                width,
                message.clone()
            )
            .is_ok()
        );
    }
    for output in [
        "",
        "keep\t../pack\n",
        &format!("keep\t{}\ntrailer", "c".repeat(40)),
        &format!("keep\t{}\n", "C".repeat(40)),
        &format!("keep\t{}\n", "c".repeat(64)),
    ] {
        assert!(created_keep(output.as_bytes(), 40, message.clone()).is_err());
    }
}
