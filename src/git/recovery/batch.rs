//! Native object IDs in; size-delimited byte frames out. Never line-split blob contents.
use super::{Reader, valid_oid};

pub(super) struct Checked {
    pub oid: String,
    pub info: Option<(String, usize)>,
}
fn input(ids: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for oid in ids {
        let oid = oid.as_ref();
        if !valid_oid(oid) {
            return Err(crate::i18n::text("backend-invalid-batch-object-id-1f74f1").into());
        }
        bytes.extend_from_slice(oid.as_bytes());
        bytes.push(b'\n');
    }
    Ok(bytes)
}
pub(super) fn check(reader: &Reader<'_>, ids: &[String]) -> Result<Vec<Checked>, String> {
    let mut results = Vec::new();
    for group in ids.chunks(128) {
        let out = reader.run(&["cat-file", "--batch-check"], Some(&input(group)?))?;
        let text = std::str::from_utf8(&out)
            .map_err(|_| crate::i18n::text("backend-invalid-batch-object-metadata-3e59c5"))?;
        let rows: Vec<_> = text.lines().collect();
        if rows.len() != group.len() || (!out.is_empty() && !out.ends_with(b"\n")) {
            return Err(crate::i18n::text("backend-truncated-batch-metadata-b539a8").into());
        }
        for (oid, row) in group.iter().zip(rows) {
            let fields: Vec<_> = row.split(' ').collect();
            let info = match fields.as_slice() {
                [actual, "missing"] if actual == oid => None,
                [actual, kind, size]
                    if actual == oid
                        && matches!(*kind, "blob" | "commit" | "tree" | "tag")
                        && !size.is_empty()
                        && size.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    Some((
                        (*kind).to_owned(),
                        size.parse::<usize>().map_err(|_| {
                            crate::i18n::text("backend-object-size-overflow-e2837f")
                        })?,
                    ))
                }
                _ => {
                    return Err(crate::i18n::text("backend-malformed-batch-metadata-2f6f82").into());
                }
            };
            results.push(Checked {
                oid: oid.clone(),
                info,
            });
        }
    }
    Ok(results)
}
pub(super) fn groups(requests: &[(String, usize)]) -> Vec<&[(String, usize)]> {
    let mut groups = Vec::new();
    let mut from = 0;
    let mut bytes = 0;
    for (i, (_, size)) in requests.iter().enumerate() {
        if i > from && bytes + size + 128 > 4 * 1024 * 1024 {
            groups.push(&requests[from..i]);
            from = i;
            bytes = 0;
        }
        bytes += size + 128;
    }
    if from < requests.len() {
        groups.push(&requests[from..]);
    }
    groups
}
pub(super) fn read(
    reader: &Reader<'_>,
    requests: &[(String, usize)],
    kind: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let out = reader.run(
        &["cat-file", "--batch"],
        Some(&input(requests.iter().map(|(oid, _)| oid))?),
    )?;
    frames(&out, requests, kind)
}
fn frames(
    out: &[u8],
    requests: &[(String, usize)],
    kind: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut at = 0;
    let mut objects = Vec::new();
    for (oid, size) in requests {
        if *size > super::BLOB_LIMIT {
            return Err(
                crate::i18n::text("backend-content-batch-exceeds-per-object-limit-da4d59").into(),
            );
        }
        let tail = out
            .get(at..)
            .ok_or(crate::i18n::text("backend-truncated-content-frame-d19b14"))?;
        let end = tail
            .iter()
            .take(256)
            .position(|b| *b == b'\n')
            .ok_or(crate::i18n::text(
                "backend-invalid-content-frame-header-c77eee",
            ))?;
        let header = std::str::from_utf8(&tail[..end])
            .map_err(|_| crate::i18n::text("backend-invalid-content-header-6bf865"))?;
        if header != format!("{oid} {kind} {size}") {
            return Err(crate::i18n::text(
                "backend-object-changed-or-content-batch-is-malformed-c3a322",
            )
            .into());
        }
        at = at
            .checked_add(end + 1)
            .ok_or(crate::i18n::text("backend-content-range-overflow-aca589"))?;
        let end = at
            .checked_add(*size)
            .ok_or(crate::i18n::text("backend-content-size-overflow-facc93"))?;
        let bytes = out.get(at..end).ok_or(crate::i18n::text(
            "backend-truncated-object-contents-2122df",
        ))?;
        if out.get(end) != Some(&b'\n') {
            return Err(crate::i18n::text("backend-missing-object-frame-separator-5ff7b5").into());
        }
        objects.push((oid.clone(), bytes.to_vec()));
        at = end + 1;
    }
    if at != out.len() {
        return Err(crate::i18n::text("backend-trailing-data-after-content-batch-891e65").into());
    }
    Ok(objects)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_frames_keep_newlines_and_nuls_and_reject_range_type_and_identity_confusion() {
        let oid = "a".repeat(40);
        let requests = vec![(oid.clone(), 4)];
        let mut out = format!("{oid} blob 4\n").into_bytes();
        out.extend_from_slice(b"a\n\0b\n");
        assert_eq!(
            frames(&out, &requests, "blob").unwrap_or_default()[0].1,
            b"a\n\0b"
        );
        assert!(frames(&out[..out.len() - 1], &requests, "blob").is_err());
        assert!(frames(&out, &requests, "commit").is_err());
        assert!(frames(&out, &[("b".repeat(40), 4)], "blob").is_err());
        assert!(frames(&out, &[(oid, usize::MAX)], "blob").is_err());
        out.push(0);
        assert!(frames(&out, &requests, "blob").is_err());
        assert!(input(["--filters"]).is_err());
    }
}
