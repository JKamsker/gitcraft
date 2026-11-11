//! Verified object import and transactional ref publication through a recording backend.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Store {
    Quarantine,
    Destination,
}

pub(super) trait Backend {
    fn git(&mut self, store: Store, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>, String>;
    fn unchanged(&mut self) -> Result<(), String>;
    fn keep_message(&mut self) -> Result<String, String>;
    fn retire_keep(&mut self, keep: &Keep) -> Result<(), String>;
}

pub(super) struct Keep {
    pub(super) hash: String,
    pub(super) message: String,
}

pub(super) fn created_keep(output: &[u8], width: usize, message: String) -> Result<Keep, String> {
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

#[cfg(test)]
pub(super) fn pipeline(
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

pub(super) fn pipeline_with_presentation(
    backend: &mut impl Backend,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
    presentation: &mut Option<super::super::presentation::Presentation>,
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

pub(super) fn require_objects(
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
