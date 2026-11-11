//! Captured routing configuration rendering and reference validation.

pub(super) fn branch_tracking(
    entries: Vec<(String, String)>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (key, value) in entries {
        let lower = key.to_ascii_lowercase();
        if lower.starts_with("branch.") && (lower.ends_with(".remote") || lower.ends_with(".merge"))
        {
            map.entry(key).or_default().push(value);
        }
    }
    map
}

pub(super) fn freeze_routing(
    mut entries: Vec<(String, String)>,
    endpoints: Vec<(String, String)>,
) -> Vec<(String, String)> {
    entries.retain(|(key, _)| {
        let lower = key.to_ascii_lowercase();
        !(lower.starts_with("url.")
            || lower.starts_with("include.")
            || lower.starts_with("includeif.")
            || (lower.starts_with("remote.")
                && (lower.ends_with(".url") || lower.ends_with(".pushurl"))))
    });
    entries.extend(endpoints);
    entries
}

pub(super) fn parse_config(config: &str) -> Result<Vec<(String, String)>, String> {
    config
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|row| {
            let (key, value) = row.split_once('\n').unwrap_or((row, "true"));
            if key.is_empty() {
                return Err(crate::i18n::text(
                    "backend-invalid-captured-git-configuration-key-4a916d",
                )
                .into());
            }
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}

pub(super) fn quote(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\u{8}', "\\b")
    )
}

pub(super) fn render_config(
    mut entries: Vec<(String, String)>,
    worktree: Option<&str>,
    hooks: &str,
    object_format: &str,
) -> Result<String, String> {
    if !matches!(object_format, "sha1" | "sha256") {
        return Err(crate::i18n::text("backend-unsupported-git-object-format-696069").into());
    }
    entries.retain(|(k, _)| {
        !matches!(
            k.to_ascii_lowercase().as_str(),
            "core.bare"
                | "core.worktree"
                | "core.hookspath"
                | "core.repositoryformatversion"
                | "extensions.worktreeconfig"
                | "extensions.objectformat"
        )
    });
    entries.extend([
        ("core.bare".into(), worktree.is_none().to_string()),
        ("core.hooksPath".into(), hooks.into()),
        (
            "core.repositoryFormatVersion".into(),
            if object_format == "sha256" { "1" } else { "0" }.into(),
        ),
    ]);
    if object_format == "sha256" {
        entries.push(("extensions.objectFormat".into(), "sha256".into()));
    }
    if let Some(path) = worktree {
        entries.push(("core.worktree".into(), path.into()));
    }
    let mut out = String::new();
    for (key, value) in entries {
        let (section, rest) = key.split_once('.').ok_or(crate::i18n::text(
            "backend-invalid-captured-config-section-377e0c",
        ))?;
        let (subsection, variable) = rest
            .rsplit_once('.')
            .map_or((None, rest), |(s, v)| (Some(s), v));
        if !section
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !variable
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.contains('\0')
        {
            return Err(
                crate::i18n::text("backend-invalid-captured-configuration-value-614893").into(),
            );
        }
        out.push_str(&format!(
            "[{section}{}]\n\t{variable} = {}\n",
            subsection
                .map(|s| format!(" {}", quote(s)))
                .unwrap_or_default(),
            quote(&value)
        ));
    }
    Ok(out)
}

pub(super) fn safe_reference(reference: &str) -> bool {
    reference.starts_with("refs/")
        && !reference.contains(['\\', '\0', '\n', '\r'])
        && reference
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

pub(super) fn object_id(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
