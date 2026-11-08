//! Bundle quarantine uses the existing machine-side workspace claim and durable owners.
//! Only the final expected-absent transaction publishes refs; no checkout or fetch.
use super::{
    Machine, runner,
    workspace::{self, Workspace},
};
use crate::git::BundleRef;
use crate::{git, remote};
use std::path::Path;
use std::time::Duration;

pub(crate) fn identity(repo: &Path) -> Result<String, String> {
    workspace::canonical_common(repo)
}

pub(crate) fn run(
    repo: &Path,
    expected: &str,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
) -> Result<(), String> {
    run_with_presentation(
        repo,
        expected,
        pack,
        refs,
        prerequisites,
        mapped,
        import,
        &mut None,
    )
}

pub(crate) fn run_with_presentation(
    repo: &Path,
    expected: &str,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    *presentation = None;
    if !remote::is_remote(repo) && !cfg!(any(target_os = "linux", windows)) {
        return Err("This platform cannot establish enforced bundle memory limits. Import/unpack is supported on Linux (including SSH/WSL) and Windows; inspection and export remain available. No pack was opened.".into());
    }
    if identity(repo)? != expected {
        return Err(
            "The destination repository changed since inspection; reopen the bundle".into(),
        );
    }
    let mut ws = Workspace::open(repo, &Machine::of(repo), None)?;
    // Every fallible post-claim path reaches explicit close so cleanup errors cannot
    // disappear through Workspace::Drop while a setup error is being propagated.
    let result = (|| {
        let common = identity(&ws.repo)?;
        if common != expected {
            return Err("The destination repository changed before quarantine setup".into());
        }
        let mut backend = Native {
            common,
            quarantine: format!("{}/bundle-quarantine.git", ws.base_there),
            ws: &ws,
        };
        pipeline_with_presentation(
            &mut backend,
            pack,
            refs,
            prerequisites,
            mapped,
            import,
            presentation,
        )
    })();
    let mut cleanup_presentation = None;
    let cleanup = ws.close_with_presentation(&mut cleanup_presentation);
    completed_with_presentation(result, cleanup, import, presentation, cleanup_presentation)
}

pub(crate) fn completed_with_presentation(
    result: Result<(), String>,
    cleanup: Result<(), String>,
    import: bool,
    presentation: &mut Option<super::presentation::Presentation>,
    cleanup_presentation: Option<super::presentation::Presentation>,
) -> Result<(), String> {
    use super::presentation::{Cleanup, Presentation};
    let context = match (&result, &cleanup) {
        (Err(primary), Err(cleanup)) => Some(Cleanup::BundleCompleted {
            primary: primary.clone(),
            cleanup: cleanup.clone(),
        }),
        _ => None,
    };
    let cleanup_only = result
        .is_ok()
        .then(|| cleanup.as_ref().err().cloned())
        .flatten();
    let result = completed(result, cleanup, import);
    if let (Err(raw), Some(context)) = (&result, context) {
        *presentation = Some(Presentation::capture(raw.clone(), context));
    } else if let (Err(raw), Some(cleanup)) = (&result, cleanup_only) {
        *presentation = Some(Presentation::completed(
            raw.clone(),
            import,
            cleanup,
            cleanup_presentation,
        ));
    }
    super::presentation::retain_error(&result, presentation);
    result
}

fn completed(
    result: Result<(), String>,
    cleanup: Result<(), String>,
    import: bool,
) -> Result<(), String> {
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(format!(
            "{} completed, but quarantine cleanup failed: {cleanup}",
            if import {
                "Ref publication"
            } else {
                "Verification"
            }
        )),
        (Err(primary), Err(cleanup)) => {
            Err(format!("{primary}; quarantine cleanup failed: {cleanup}"))
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Store {
    Quarantine,
    Destination,
}
trait Backend {
    fn git(&mut self, store: Store, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>, String>;
    fn unchanged(&mut self) -> Result<(), String>;
    fn keep_message(&mut self) -> Result<String, String>;
    fn retire_keep(&mut self, keep: &Keep) -> Result<(), String>;
}
struct Keep {
    hash: String,
    message: String,
}
fn created_keep(output: &[u8], width: usize, message: String) -> Result<Keep, String> {
    let text = std::str::from_utf8(output)
        .map_err(|_| "Unconfirmed pack keep response; protection retained")?;
    let (kind, hash) = text
        .strip_suffix('\n')
        .and_then(|s| s.split_once('\t'))
        .ok_or("Unconfirmed pack keep response; protection retained")?;
    if hash.len() != width
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Unconfirmed pack hash; protection retained".into());
    }
    match kind {
        "keep" => Ok(Keep {
            hash: hash.into(),
            message,
        }),
        // Git reports `pack` if --keep found an existing marker. Its owner could
        // retire it during our transaction: neither borrow it nor remove it.
        "pack" => Err(format!(
            "This pack already has another keep marker (pack-{hash}.keep); it was left untouched and no refs were published. Inspect its owner/recovery evidence before retrying."
        )),
        _ => Err("Unconfirmed pack keep response; protection retained".into()),
    }
}
fn pipeline(
    backend: &mut impl Backend,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
) -> Result<(), String> {
    pipeline_with_presentation(
        backend,
        pack,
        refs,
        prerequisites,
        mapped,
        import,
        &mut None,
    )
}

fn pipeline_with_presentation(
    backend: &mut impl Backend,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    *presentation = None;
    let width = refs.first().ok_or("No advertised bundle refs")?.oid.len();
    let format = match width {
        40 => "sha1",
        64 => "sha256",
        _ => return Err("Unsupported object format".into()),
    };
    let actual = backend.git(
        Store::Destination,
        &["rev-parse", "--show-object-format"],
        vec![],
    )?;
    if std::str::from_utf8(&actual).map(str::trim) != Ok(format)
        || refs
            .iter()
            .chain(prerequisites)
            .any(|r| r.oid.len() != width)
    {
        return Err("Bundle and destination object formats differ".into());
    }
    backend.git(
        Store::Quarantine,
        &[
            "init",
            "--bare",
            "--template=",
            &format!("--object-format={format}"),
        ],
        vec![],
    )?;
    require_objects(backend, Store::Quarantine, prerequisites, true)?;
    let index = [
        "index-pack",
        "--stdin",
        "--fix-thin",
        "--strict",
        "--threads=1",
        "--max-input-size=134217728",
    ];
    backend.git(Store::Quarantine, &index, pack.to_vec())?;
    let mut fsck = vec![
        "fsck",
        "--full",
        "--strict",
        "--no-reflogs",
        "--no-progress",
    ];
    fsck.extend(refs.iter().chain(prerequisites).map(|r| r.oid.as_str()));
    backend.git(Store::Quarantine, &fsck, vec![])?;
    require_objects(backend, Store::Quarantine, refs, false)?;
    backend.unchanged()?;
    if !import {
        return Ok(());
    }
    // Reinstall the exact held pack under the same enforced memory limit. No Git URL,
    // transport, temporary source ref, force update or FETCH_HEAD is involved.
    let message = backend.keep_message()?;
    let keep_option = format!("--keep={message}");
    let protected_index: Vec<_> = index
        .iter()
        .copied()
        .chain([keep_option.as_str()])
        .collect();
    let receipt = backend
        .git(Store::Destination, &protected_index, pack.to_vec())
        .map_err(|e| format!("{e}; any created destination pack keep protection was retained"))?;
    let keep = created_keep(&receipt, width, message)?;
    // Deliberately no Drop/failure cleanup. Any lost terminal acknowledgement,
    // malformed output, object-check or publication failure retains Git protection.
    (|| {
        require_objects(backend, Store::Destination, prerequisites, true)?;
        require_objects(backend, Store::Destination, refs, false)?;
        backend.unchanged()?;
        let mut transaction = String::from("start\n");
        for r in mapped {
            transaction.push_str(&format!("create {} {}\n", r.name, r.oid));
        }
        transaction.push_str("prepare\ncommit\n");
        backend.git(
            Store::Destination,
            &["update-ref", "--no-deref", "--stdin"],
            transaction.into_bytes(),
        )?;
        Ok::<(), String>(())
    })()
    .map_err(|e| keep_retained(e, &keep.hash, presentation))?;
    // The existing owned Git call returned a successful, confirmed terminal outcome.
    // Only now can refs protect these objects instead of this import's keep marker.
    backend.retire_keep(&keep).map_err(|e| format!("Ref publication completed, but owned pack keep retirement failed (protection may remain): {e}"))?;
    Ok(())
}
fn require_objects(
    backend: &mut impl Backend,
    store: Store,
    refs: &[BundleRef],
    commits: bool,
) -> Result<(), String> {
    if refs.is_empty() {
        return Ok(());
    }
    let mut input = String::new();
    for r in refs {
        input.push_str(&r.oid);
        if commits {
            input.push_str("^{commit}");
        }
        input.push('\n');
    }
    let output = backend.git(store, &["cat-file", "--batch-check"], input.into_bytes())?;
    let text =
        std::str::from_utf8(&output).map_err(|_| "Object verification response was not UTF-8")?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != refs.len() {
        return Err("Incomplete object verification response".into());
    }
    for (line, expected) in lines.iter().zip(refs) {
        let words: Vec<_> = line.split_ascii_whitespace().collect();
        if words.len() != 3
            || words[0] != expected.oid
            || (commits && words[1] != "commit")
            || !matches!(words[1], "commit" | "tree" | "blob" | "tag")
            || words[2].parse::<u64>().is_err()
        {
            return Err(format!(
                "Required object {} is missing or has an unexpected type; no network fetch was allowed",
                expected.oid
            ));
        }
    }
    Ok(())
}

struct Native<'a> {
    ws: &'a Workspace,
    common: String,
    quarantine: String,
}
impl Backend for Native<'_> {
    fn keep_message(&mut self) -> Result<String, String> {
        let mut nonce = [0; 16];
        gitcraft_os::random_bytes(&mut nonce).map_err(|e| e.to_string())?;
        Ok(format!(
            "gitcraft-bundle-keep-v1-{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ))
    }
    fn retire_keep(&mut self, keep: &Keep) -> Result<(), String> {
        let words = if remote::is_remote(&self.ws.repo) {
            runner::sh_alias(
                "gitcraft-bundle-retire",
                "exec python3 -c \"$@\"",
                &[
                    crate::git::bundle_helper::RETIRE_KEEP_REMOTE,
                    &self.common,
                    &keep.hash,
                    &keep.message,
                ],
            )
        } else {
            let args = vec![keep.hash.clone(), keep.message.clone()];
            helper_words("--retire-owned-keep", &self.common, &args)?
        };
        self.execute(&words, vec![]).map(|_| ())
    }
    fn unchanged(&mut self) -> Result<(), String> {
        if identity(&self.ws.repo)? != self.common {
            return Err("The destination repository changed; refs were not published".into());
        }
        self.ws
            .authority(remote::workspace_authority::FenceMode::Mutation)
            .map(|_| ())
    }
    fn git(&mut self, store: Store, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>, String> {
        let directory = if store == Store::Destination {
            &self.common
        } else {
            &self.quarantine
        };
        let alternate = if store == Store::Quarantine {
            let objects = format!("{}/objects", self.common);
            if objects.chars().any(char::is_control) {
                return Err("Object directory has unsupported control characters".into());
            }
            serde_json::to_string(&objects).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        let mut command = policy(directory, &format!("{}/no-hooks", self.ws.base_there));
        command.extend(args.iter().map(|s| (*s).to_owned()));
        let words = wrapper(remote::is_remote(&self.ws.repo), &alternate, &command)?;
        self.execute(&words, input)
    }
}
impl Native<'_> {
    fn execute(&self, words: &[String], input: Vec<u8>) -> Result<Vec<u8>, String> {
        let words: Vec<_> = words.iter().map(String::as_str).collect();
        let authority = self
            .ws
            .authority(remote::workspace_authority::FenceMode::Mutation)?;
        let record = self.ws.owner_record(&authority)?;
        let mut owned = git::WorkspaceCommand::new(&authority);
        owned.input = Some(input);
        let out = self.ws.execute(
            &self.ws.repo,
            &words,
            &record,
            Duration::from_secs(120),
            owned,
        )?;
        if out.stdout.len() > 2 << 20 || out.stderr.len() > 2 << 20 {
            return Err(
                "Contained Git output exceeded 2 MiB; no partial success is accepted".into(),
            );
        }
        if out.code != Some(0) {
            return Err(format!(
                "Contained Git failed ({}): {}",
                out.code.map_or("no exit code".into(), |n| n.to_string()),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out.stdout)
    }
}
fn policy(directory: &str, hooks: &str) -> Vec<String> {
    [
        "--no-lazy-fetch",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "-c",
        "core.commitGraph=false",
        "-c",
        "core.multiPackIndex=false",
        "-c",
        "protocol.allow=never",
        "-c",
        "credential.helper=",
        "-c",
        "pack.threads=1",
        "-c",
        "pack.windowMemory=32m",
        "-c",
        "pack.deltaCacheSize=32m",
        "-c",
        "core.packedGitWindowSize=8m",
        "-c",
        "core.packedGitLimit=64m",
        "-c",
        "core.bigFileThreshold=16m",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([
        "-c".into(),
        format!("core.hooksPath={hooks}"),
        format!("--git-dir={directory}"),
    ])
    .collect()
}
fn wrapper(there: bool, alternate: &str, args: &[String]) -> Result<Vec<String>, String> {
    let mut owned = Vec::new();
    let script = if there {
        owned.push(crate::git::bundle_helper::REMOTE.to_owned());
        owned.push(alternate.to_owned());
        "command -v python3 >/dev/null 2>&1 || { echo 'Contained bundle import requires Python 3 on this Linux host; no pack was opened.' >&2; exit 78; }; exec python3 -c \"$@\""
    } else {
        return helper_words(&crate::git::bundle_helper::program()?, alternate, args);
    };
    owned.extend_from_slice(args);
    let borrowed: Vec<_> = owned.iter().map(String::as_str).collect();
    Ok(runner::sh_alias(
        "gitcraft-bundle-contained",
        script,
        &borrowed,
    ))
}
fn helper_words(program: &str, alternate: &str, args: &[String]) -> Result<Vec<String>, String> {
    let mut owned = Vec::new();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let exe = crate::app::strip_verbatim(exe);
    let exe = exe
        .to_str()
        .ok_or("Application path is not UTF-8")?
        .to_owned();
    #[cfg(windows)]
    let exe = exe.replace('\\', "/");
    owned.push(exe);
    owned.push(program.into());
    owned.push(alternate.to_owned());
    let script = "helper=$1; git=$2; alternate=$3; shift 3; exec \"$helper\" --gitcraft-contained-git \"$git\" \"$alternate\" -- \"$@\"";
    owned.extend_from_slice(args);
    let borrowed: Vec<_> = owned.iter().map(String::as_str).collect();
    Ok(runner::sh_alias(
        "gitcraft-bundle-contained",
        script,
        &borrowed,
    ))
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[derive(Default)]
    struct Recording {
        calls: Vec<(Store, Vec<String>, Vec<u8>)>,
        fail: Option<String>,
        changed: bool,
        published: bool,
        marker: Option<String>,
        preexisting: bool,
        stage: Option<&'static str>,
        events: Vec<&'static str>,
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
    fn every_git_command_has_no_lazy_no_network_no_hooks_and_fixed_memory_policy()
    -> Result<(), String> {
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
}

#[cfg(test)]
mod execution_fluent_r1_tests {
    use super::completed;
    use crate::parallel_bisect::display::{CleanupContext, cleanup_diagnostic, diagnostic};

    #[test]
    fn actual_completed_results_need_separate_cleanup_context_for_presentation()
    -> Result<(), String> {
        // Only this pure Result combinator is called: no backend, workspace, Git or helper.
        for (locale, statement) in [
            ("en-US", "quarantine cleanup failed"),
            ("de", "die Quarantäne konnte nicht bereinigt werden"),
        ] {
            crate::i18n::set_language(locale);
            for primary in [
                "Contained Git failed (1): object denied",
                "Contained Git failed (1): object denied; quarantine cleanup failed: provider detail (exit 7); stderr: raw\n",
            ] {
                let cleanup =
                    "cleanup denied; quarantine cleanup failed: provider detail\n$HOME {detail}";
                for import in [false, true] {
                    let result = completed(Err(primary.into()), Err(cleanup.into()), import);
                    let raw = result.err().ok_or("expected combined failure")?;
                    assert_eq!(
                        raw,
                        format!("{primary}; quarantine cleanup failed: {cleanup}")
                    );
                    let shown = cleanup_diagnostic(
                        &raw,
                        CleanupContext::BundleCompleted { primary, cleanup },
                    );
                    assert_eq!(shown, format!("{primary}; {statement}: {cleanup}"));
                    assert!(shown.contains(primary) && shown.contains(cleanup));
                    // Rendering borrows the error; the original Result payload is retained.
                    assert_eq!(
                        raw,
                        format!("{primary}; quarantine cleanup failed: {cleanup}")
                    );
                }
            }
            for unknown in [
                "provider text; quarantine cleanup failed: provider detail",
                "provider text; workspace cleanup: provider detail",
                "provider text; owned pack-01234567.keep retained",
            ] {
                let raw = completed(Err(unknown.into()), Ok(()), true)
                    .err()
                    .ok_or("expected propagated primary")?;
                assert_eq!(raw, unknown);
                assert_eq!(diagnostic(&raw), unknown);
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}

pub(crate) fn keep_retained(
    primary: String,
    hash: &str,
    presentation: &mut Option<super::presentation::Presentation>,
) -> String {
    let raw = format!("{primary}; owned pack-{hash}.keep retained");
    *presentation = Some(super::presentation::Presentation::capture(
        raw.clone(),
        super::presentation::Cleanup::BundleKeepRetained {
            primary,
            hash: hash.into(),
        },
    ));
    raw
}
