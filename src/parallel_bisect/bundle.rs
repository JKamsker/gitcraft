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
        pipeline(&mut backend, pack, refs, prerequisites, mapped, import)
    })();
    completed(result, ws.close(), import)
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
}
fn pipeline(
    backend: &mut impl Backend,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
) -> Result<(), String> {
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
    backend.git(Store::Destination, &index, pack.to_vec())?;
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
        owned.push(crate::git::bundle_helper::program()?);
        owned.push(alternate.to_owned());
        "helper=$1; git=$2; alternate=$3; shift 3; exec \"$helper\" --gitcraft-contained-git \"$git\" \"$alternate\" -- \"$@\""
    };
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
    }
    impl Backend for Recording {
        fn unchanged(&mut self) -> Result<(), String> {
            if self.changed {
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
            Ok(match args[0] {
                "rev-parse" => b"sha1\n".to_vec(),
                "cat-file" => String::from_utf8(input)
                    .unwrap()
                    .lines()
                    .map(|l| format!("{} commit 10\n", l.trim_end_matches("^{commit}")))
                    .collect::<String>()
                    .into_bytes(),
                "update-ref" => {
                    self.published = true;
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
    fn verification_never_publishes_and_import_uses_identical_pack_then_atomic_create() {
        let refs = refs();
        let mapped = vec![BundleRef {
            name: "refs/bundle-import/test/heads/main".into(),
            oid: refs[0].oid.clone(),
        }];
        let mut read = Recording::default();
        pipeline(&mut read, b"PACK-recording", &refs, &[], &mapped, false).unwrap();
        assert!(!read.published && read.calls.iter().all(|(_, a, _)| a[0] != "update-ref"));
        let mut write = Recording::default();
        pipeline(&mut write, b"PACK-recording", &refs, &[], &mapped, true).unwrap();
        let packs: Vec<_> = write
            .calls
            .iter()
            .filter(|(_, a, _)| a[0] == "index-pack")
            .collect();
        assert_eq!(packs.len(), 2);
        assert_eq!(packs[0].2, packs[1].2);
        let (_, args, input) = write.calls.last().unwrap();
        assert_eq!(args, &["update-ref", "--no-deref", "--stdin"]);
        assert_eq!(
            String::from_utf8(input.clone()).unwrap(),
            format!(
                "start\ncreate {} {}\nprepare\ncommit\n",
                mapped[0].name, mapped[0].oid
            )
        );
        assert!(write.published);
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
                    .unwrap_err()
                    .contains("unconfirmed")
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
            .unwrap_err()
            .contains("unconfirmed; quarantine cleanup failed: ledger retained")
        );
        assert!(
            completed(Ok(()), Err("ref cleanup failed".into()), true)
                .unwrap_err()
                .starts_with("Ref publication completed")
        );
    }
    #[test]
    fn every_git_command_has_no_lazy_no_network_no_hooks_and_fixed_memory_policy() {
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
        let words = wrapper(true, "\"/objects\"", &args).unwrap();
        assert!(words.iter().any(|s| s.contains("resource.setrlimit")));
        assert!(words.iter().all(|s| !s.contains("${@:3}")));
    }
}
