//! Bounded bundle header parsing and untrusted reference validation.

use super::*;

pub(super) const HEADER_LIMIT: usize = 1024 * 1024;

pub(super) const REF_LIMIT: usize = 1000;

pub(super) fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid.bytes().all(|b| b.is_ascii_hexdigit())
        && oid.bytes().any(|b| b != b'0')
}

pub(super) fn valid_ref(name: &str) -> Result<(), String> {
    // Git's full-ref grammar is repository-independent. Validate locally so an
    // inspection never needs one remote round trip per advertised ref.
    if name == "HEAD" {
        return Ok(());
    }
    let invalid = name.len() > 1024
        || !name.starts_with("refs/")
        || name.ends_with(['/', '.'])
        || name.contains("..")
        || name.contains("@{")
        || name
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
        || name
            .bytes()
            .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\".contains(&b));
    if invalid {
        Err(crate::i18n::message(
            "backend-invalid-bundle-ref-name-897fdb",
            &[("name", (name).to_string().into())],
        ))
    } else {
        Ok(())
    }
}

pub(super) fn header(bytes: &[u8]) -> Result<(Vec<BundleRef>, Vec<BundleRef>), String> {
    if bytes.len() > BUNDLE_LIMIT {
        return Err(crate::i18n::text("backend-bundle-exceeds-128-mib-c505c5").into());
    }
    let mut pos = 0;
    let mut next = || -> Result<std::borrow::Cow<'_, str>, String> {
        let end = bytes
            .get(pos..)
            .and_then(|b| b.iter().position(|c| *c == b'\n'))
            .ok_or(crate::i18n::text("backend-incomplete-bundle-header-3f723b"))?
            + pos;
        if end > HEADER_LIMIT {
            return Err(crate::i18n::text("backend-bundle-header-exceeds-1-mib-868fea").into());
        }
        let line = &bytes[pos..end];
        pos = end + 1;
        if line.starts_with(b"-") {
            // Subjects can come from legacy-encoded commits; the OID is still validated.
            Ok(String::from_utf8_lossy(line))
        } else {
            std::str::from_utf8(line)
                .map(std::borrow::Cow::Borrowed)
                .map_err(|_| {
                    crate::i18n::text("backend-bundle-ref-header-is-not-utf-8-a72a69").to_owned()
                })
        }
    };
    let version = next()?;
    if !matches!(version.as_ref(), "# v2 git bundle" | "# v3 git bundle") {
        return Err(crate::i18n::text("backend-not-a-supported-git-v2-v3-bundle-d8d741").into());
    }
    let (mut refs, mut prerequisites) = (Vec::new(), Vec::new());
    let mut names = BTreeSet::new();
    let mut hash_len = 40;
    loop {
        let line = next()?;
        if line.is_empty() {
            break;
        }
        if let Some(capability) = line.strip_prefix('@') {
            if version != "# v3 git bundle" || !refs.is_empty() || !prerequisites.is_empty() {
                return Err(crate::i18n::text("backend-misplaced-bundle-capability-92bb13").into());
            }
            hash_len = match capability {
                "object-format=sha1" => 40,
                "object-format=sha256" => 64,
                _ => {
                    return Err(crate::i18n::message(
                        "backend-unsupported-bundle-capability-capability-644a2c",
                        &[("capability", (capability).to_string().into())],
                    ));
                }
            };
            continue;
        }
        let (prerequisite, line) = match line.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, line.as_ref()),
        };
        let (oid, name) = match line.split_once(' ') {
            Some(parts) => parts,
            None if prerequisite => (line, ""),
            None => {
                return Err(crate::i18n::text("backend-invalid-bundle-header-entry-a00585").into());
            }
        };
        if !valid_oid(oid)
            || oid.len() != hash_len
            || (!prerequisite && (name.is_empty() || name.chars().any(char::is_control)))
        {
            return Err(
                crate::i18n::text("backend-invalid-bundle-object-id-or-name-776557").into(),
            );
        }
        let r = BundleRef {
            // Prerequisite descriptions are commit subjects, not ref names. Empty
            // subjects and tabs are valid; sanitize controls only for presentation.
            name: if prerequisite {
                name.chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect()
            } else {
                name.into()
            },
            oid: oid.to_ascii_lowercase(),
        };
        if prerequisite {
            prerequisites.push(r);
        } else {
            if !names.insert(name.to_owned()) {
                return Err(
                    crate::i18n::text("backend-duplicate-advertised-bundle-ref-c6d994").into(),
                );
            }
            refs.push(r);
        }
        if refs.len() + prerequisites.len() > REF_LIMIT {
            return Err(crate::i18n::text(
                "backend-bundle-has-more-than-1000-refs-prerequisites-73ceb6",
            )
            .into());
        }
    }
    if refs.is_empty() || bytes.get(pos..pos + 4) != Some(b"PACK") {
        return Err(crate::i18n::text(
            "backend-bundle-has-no-advertised-refs-or-no-pack-data-71b03c",
        )
        .into());
    }
    Ok((refs, prerequisites))
}
