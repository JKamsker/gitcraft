//! Bounded reachable-object scans for literal replacement residuals.

use super::*;

// Fixed harmless argv; literal rules never leave memory during residual inspection.
// Enumerate metadata first so a large blob cannot force an unbounded batch allocation.
pub(super) const SCAN_OBJECTS: usize = 20_000;

pub(super) const SCAN_BLOB: usize = 4 << 20;

pub(super) const SCAN_TOTAL: usize = 16 << 20;

pub(super) fn residual_checks(
    config: &Config,
    mut run: impl FnMut(&[&str], Vec<u8>, usize) -> Result<Vec<u8>, String>,
) -> Vec<Check> {
    if config.replacements.is_empty() {
        return Vec::new();
    }
    let result = scan_residuals(config, &mut run);
    config.replacements.iter().enumerate().map(|(i, _)| {
        let (passed, detail) = match &result {
            Ok((hits, _)) if hits[i] => (false, crate::i18n::text("backend-literal-find-text-remains-in-reachable-text-blobs-inspect-before-dabdf8").into()),
            Ok((_, true)) => (false, crate::i18n::text("backend-unverified-binary-blobs-were-skipped-nul-in-first-8192-bytes-6ac1e7").into()),
            Ok(_) => (true, crate::i18n::text("backend-no-literal-find-text-in-reachable-text-blobs-at-most-20000-object-6fd071").into()),
            Err(error) => (false, crate::i18n::message("backend-unverified-79e7cc", &[("config", (config.redact(error)).to_string().into())])),
        };
        Check { label: crate::i18n::message("backend-replacement-residual-search-99ed56", &[("index", (i + 1).to_string().into())]), passed, detail }
    }).collect()
}

pub(super) fn scan_residuals(
    config: &Config,
    run: &mut impl FnMut(&[&str], Vec<u8>, usize) -> Result<Vec<u8>, String>,
) -> Result<(Vec<bool>, bool), String> {
    let objects = run(
        &["rev-list", "--objects", "--all", "--no-object-names"],
        Vec::new(),
        LIMIT,
    )?;
    let text = std::str::from_utf8(&objects)
        .map_err(|_| crate::i18n::text("backend-invalid-reachable-object-list-1be452"))?;
    let mut ids = std::collections::BTreeSet::new();
    for id in text.lines() {
        if !valid_object_id(id) {
            return Err(crate::i18n::text("backend-invalid-reachable-object-id-413f0f").into());
        }
        ids.insert(id);
        if ids.len() > SCAN_OBJECTS {
            return Err(crate::i18n::text(
                "backend-reachable-object-scan-exceeds-20000-objects-d55e4e",
            )
            .into());
        }
    }
    if ids.is_empty() {
        return Ok((vec![false; config.replacements.len()], false));
    }
    let input: Vec<u8> = ids.iter().flat_map(|id| id.bytes().chain(*b"\n")).collect();
    let metadata = run(&["cat-file", "--batch-check"], input, LIMIT)?;
    let metadata = std::str::from_utf8(&metadata)
        .map_err(|_| crate::i18n::text("backend-invalid-object-metadata-cafe82"))?;
    let mut lines = metadata.lines();
    let mut blobs = Vec::new();
    let mut total = 0usize;
    for id in ids {
        let line = lines.next().ok_or(crate::i18n::text(
            "backend-incomplete-object-metadata-de847e",
        ))?;
        let fields: Vec<_> = line.split(' ').collect();
        if fields.len() != 3 || fields[0] != id {
            return Err(
                crate::i18n::text("backend-missing-or-mismatched-reachable-object-6c3d8d").into(),
            );
        }
        let size = fields[2]
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-object-size-8e8433"))?;
        match fields[1] {
            "blob" => {
                if size > SCAN_BLOB {
                    return Err(
                        crate::i18n::text("backend-text-scan-exceeds-4-mib-per-blob-inspect-larger-blobs-separately-4b331a").into(),
                    );
                }
                total = total.checked_add(size).ok_or(crate::i18n::text(
                    "backend-object-sizes-overflow-the-scan-budget-f89027",
                ))?;
                if total > SCAN_TOTAL {
                    return Err(
                        crate::i18n::text("backend-text-scan-exceeds-16-mib-total-inspect-the-remaining-history-sepa-a0b054")
                            .into(),
                    );
                }
                blobs.push((id.to_owned(), size));
            }
            "tree" | "commit" | "tag" => (),
            _ => {
                return Err(
                    crate::i18n::text("backend-unknown-reachable-object-type-1850cd").into(),
                );
            }
        }
    }
    if lines.next().is_some() {
        return Err(crate::i18n::text("backend-unexpected-object-metadata-d1d00f").into());
    }
    let input = blobs
        .iter()
        .flat_map(|(id, _)| id.bytes().chain(*b"\n"))
        .collect();
    if blobs.is_empty() {
        return Ok((vec![false; config.replacements.len()], false));
    }
    let bytes = run(&["cat-file", "--batch"], input, total + blobs.len() * 96)?;
    let matchers: Vec<_> = config
        .replacements
        .iter()
        .map(|rule| Literal::new(rule.find.as_bytes()))
        .collect();
    let mut hits = vec![false; matchers.len()];
    let mut skipped_binary = false;
    let mut rest = bytes.as_slice();
    for (id, size) in blobs {
        let end = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or(crate::i18n::text("backend-missing-blob-header-3b114f"))?;
        let header = format!("{id} blob {size}");
        if rest[..end] != *header.as_bytes() {
            return Err(crate::i18n::text("backend-mismatched-blob-header-41e487").into());
        }
        rest = &rest[end + 1..];
        if rest.len() <= size || rest[size] != b'\n' {
            return Err(crate::i18n::text("backend-truncated-blob-stream-24db27").into());
        }
        let blob = &rest[..size];
        if blob[..blob.len().min(8192)].contains(&0) {
            skipped_binary = true;
        } else {
            for (found, matcher) in hits.iter_mut().zip(&matchers) {
                *found |= matcher.contains(blob);
            }
        }
        rest = &rest[size + 1..];
    }
    if !rest.is_empty() {
        return Err(crate::i18n::text("backend-unexpected-blob-stream-suffix-16709a").into());
    }
    Ok((hits, skipped_binary))
}

pub(super) fn valid_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

// Linear literal matching over bytes, including non-UTF-8 text; no regex interpretation.
pub(super) struct Literal<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) prefix: Vec<usize>,
}

impl<'a> Literal<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        let mut prefix = vec![0; bytes.len()];
        let mut matched = 0;
        for i in 1..bytes.len() {
            while matched > 0 && bytes[i] != bytes[matched] {
                matched = prefix[matched - 1];
            }
            if bytes[i] == bytes[matched] {
                matched += 1;
            }
            prefix[i] = matched;
        }
        Self { bytes, prefix }
    }
    pub(super) fn contains(&self, haystack: &[u8]) -> bool {
        if self.bytes.is_empty() {
            return false;
        } // Config rejects empty find text.
        let mut matched = 0;
        for byte in haystack {
            while matched > 0 && *byte != self.bytes[matched] {
                matched = self.prefix[matched - 1];
            }
            if *byte == self.bytes[matched] {
                matched += 1;
            }
            if matched == self.bytes.len() {
                return true;
            }
        }
        false
    }
}
