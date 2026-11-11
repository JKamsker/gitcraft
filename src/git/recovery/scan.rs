//! Budgeted raw-blob selection and literal-text matching, without checkout or filters.
use super::*;

pub(super) fn scan(
    reader: &Reader<'_>,
    query: &str,
    mut blobs: BTreeMap<String, Vec<Provenance>>,
    report: &mut Report,
) -> Result<(), String> {
    let ids: Vec<_> = blobs.keys().cloned().collect();
    let checked = batch::check(reader, &ids)?;
    let mut requested = Vec::new();
    for item in checked {
        let Some((kind, size)) = item.info else {
            report.missing += 1;
            continue;
        };
        if kind != "blob" {
            return Err(crate::i18n::text(
                "backend-an-expected-content-object-changed-type-2096e4",
            )
            .into());
        }
        if size > BLOB_LIMIT {
            report.large += 1;
            continue;
        }
        if report.bytes + size > BYTE_LIMIT {
            report.limited = true;
            continue;
        }
        report.bytes += size;
        requested.push((item.oid, size));
    }
    for group in batch::groups(&requested) {
        for (oid, bytes) in batch::read(reader, group, "blob")? {
            report.scanned += 1;
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) if !text.contains('\0') => text,
                _ => {
                    report.binary += 1;
                    continue;
                }
            };
            if let Some(at) = text.find(query) {
                let (line, excerpt) = excerpt(text, at);
                let occurrences = text.match_indices(query).take(1001).count();
                if report.hits.len() >= HITS {
                    report.limited = true;
                    continue;
                }
                let mut provenance = blobs.remove(&oid).unwrap_or_default();
                provenance.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.path.cmp(&b.path)));
                report.hits.push(Hit {
                    blob: oid,
                    size: bytes.len(),
                    line,
                    excerpt,
                    occurrences,
                    provenance,
                });
            }
        }
    }
    report
        .hits
        .sort_by(|a, b| b.when().cmp(&a.when()).then_with(|| a.blob.cmp(&b.blob)));
    Ok(())
}
