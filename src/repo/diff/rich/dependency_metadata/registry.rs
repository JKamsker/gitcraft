use super::*;

pub(in crate::repo::diff::rich) fn urls(
    ecosystem: Ecosystem,
    name: &str,
    version: &str,
) -> Result<(Option<String>, String), String> {
    if name.len() > 214
        || version.len() > 128
        || version.is_empty()
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b))
    {
        return Err(crate::i18n::text("repo-core-3960").into());
    }
    match ecosystem {
        Ecosystem::Cargo
            if !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) =>
        {
            Ok((
                None,
                format!("https://static.crates.io/crates/{name}/{name}-{version}.crate"),
            ))
        }
        Ecosystem::Npm if valid_npm(name) => Ok((
            Some(format!(
                "https://registry.npmjs.org/{}/{version}",
                name.replace('/', "%2F")
            )),
            String::new(),
        )),
        _ => Err(crate::i18n::text("repo-core-3964").into()),
    }
}
pub(super) fn valid_npm(name: &str) -> bool {
    let parts: Vec<_> = name.trim_start_matches('@').split('/').collect();
    (parts.len() == 1 && !name.starts_with('@') || parts.len() == 2 && name.starts_with('@'))
        && parts.iter().all(|p| {
            !p.is_empty()
                && !matches!(*p, "." | "..")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
pub(super) fn get(url: &str) -> Result<Vec<u8>, String> {
    if network::refused().is_some() {
        return Err(crate::i18n::text("repo-core-3965").into());
    }
    if tasks::cancellation().is_cancelled() {
        return Err(crate::i18n::text("repo-core-2896").into());
    }
    let agent = crate::forge::http::with_proxy(ureq::AgentBuilder::new(), url)
        .timeout(Duration::from_secs(12))
        .redirects(0)
        .user_agent(concat!(
            "gitcraft/",
            env!("CARGO_PKG_VERSION"),
            " (package review)"
        ))
        .build();
    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(n, _) => crate::i18n::message("repo-core-3969", &[("n", (n).into())]),
        _ => crate::i18n::text("repo-core-3970").into(),
    })?;
    if response.status() != 200 {
        return Err(crate::i18n::text("repo-core-3971").into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((8 << 20) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| crate::i18n::text("repo-core-3972"))?;
    if bytes.len() > 8 << 20 {
        return Err(crate::i18n::text("repo-core-3973").into());
    }
    if tasks::cancellation().is_cancelled() {
        return Err(crate::i18n::text("repo-core-2896").into());
    }
    Ok(bytes)
}
pub(super) fn parse(
    ecosystem: Ecosystem,
    name: &str,
    version: &str,
    archive_url: String,
    bytes: &[u8],
) -> Result<Metadata, String> {
    let mut archive = archive::texts(bytes)?;
    let mut license_file = None;
    let mut license_note = None;
    let files = &mut archive.files;
    let license = match ecosystem {
        Ecosystem::Cargo => {
            let manifest = files.remove("Cargo.toml").ok_or_else(|| {
                archive
                    .skipped
                    .get("Cargo.toml")
                    .cloned()
                    .unwrap_or_else(|| crate::i18n::text("repo-core-3974").into())
            })?;
            let doc = manifest
                .parse::<toml_edit::Document<String>>()
                .map_err(|_| crate::i18n::text("repo-core-3975"))?;
            let package = doc
                .get("package")
                .ok_or(crate::i18n::text("repo-core-3976"))?;
            if package.get("name").and_then(|x| x.as_str()) != Some(name)
                || package.get("version").and_then(|x| x.as_str()) != Some(version)
            {
                return Err(crate::i18n::text("repo-core-3977").into());
            }
            if let Some(path) = package.get("license-file").and_then(|x| x.as_str()) {
                let safe = !path.is_empty()
                    && !path.starts_with('/')
                    && !path.contains('\\')
                    && !path.chars().any(char::is_control)
                    && path.split('/').all(|p| p != "..");
                let normal = path
                    .split('/')
                    .filter(|p| !matches!(*p, "" | "."))
                    .collect::<Vec<_>>()
                    .join("/");
                let content = if safe && !normal.is_empty() {
                    let mut selected = archive::texts_with(bytes, Some(&normal))?;
                    selected.files.remove(&normal).ok_or_else(|| {
                        selected
                            .skipped
                            .remove(&normal)
                            .unwrap_or_else(|| crate::i18n::text("repo-core-3979").into())
                    })
                } else {
                    Err(crate::i18n::text("repo-core-3980").into())
                };
                license_file = Some((path.to_owned(), content));
            }
            package
                .get("license")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        }
        Ecosystem::Npm => {
            let manifest: serde_json::Value =
                serde_json::from_str(&files.remove("package.json").ok_or_else(|| {
                    archive
                        .skipped
                        .get("package.json")
                        .cloned()
                        .unwrap_or_else(|| crate::i18n::text("repo-core-3981").into())
                })?)
                .map_err(|_| crate::i18n::text("repo-core-3982"))?;
            if manifest["name"].as_str() != Some(name)
                || manifest["version"].as_str() != Some(version)
            {
                return Err(crate::i18n::text("repo-core-3977").into());
            }
            let value = manifest.get("license").or_else(|| manifest.get("licenses"));
            fn declared(value: &serde_json::Value) -> Option<String> {
                match value {
                    serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                    serde_json::Value::Object(o) => o.get("type").and_then(declared),
                    serde_json::Value::Array(items) => {
                        let terms = items.iter().map(declared).collect::<Option<Vec<_>>>()?;
                        (!terms.is_empty()).then(|| terms.join(" OR "))
                    }
                    _ => None,
                }
            }
            let license = value.and_then(declared);
            if value.is_some() && license.is_none() {
                license_note = Some(crate::i18n::text("repo-core-3984").into());
            }
            license
        }
        _ => return Err(crate::i18n::text("repo-core-3985").into()),
    };
    files.remove("Cargo.toml");
    files.remove("package.json");
    Ok(Metadata {
        license,
        changelogs: std::mem::take(files),
        archive_url,
        license_file,
        license_note,
        skipped: archive.skipped,
    })
}
pub(super) fn fetch(eco: Ecosystem, registry: &Registry) -> Result<Option<Metadata>, String> {
    let Some((name, version)) = registry else {
        return Ok(None);
    };
    let (manifest, mut url) = urls(eco, name, version)?;
    if let Some(endpoint) = manifest {
        let json: serde_json::Value = serde_json::from_slice(&get(&endpoint)?)
            .map_err(|_| crate::i18n::text("repo-core-3986"))?;
        if json["name"].as_str() != Some(name) || json["version"].as_str() != Some(version) {
            return Err(crate::i18n::text("repo-core-3987").into());
        }
        url = json
            .pointer("/dist/tarball")
            .and_then(|s| s.as_str())
            .ok_or(crate::i18n::text("repo-core-3989"))?
            .to_owned();
        if !url.starts_with("https://registry.npmjs.org/") || url.contains(['?', '#', '\\']) {
            return Err(crate::i18n::text("repo-core-3990").into());
        }
    }
    parse(eco, name, version, url.clone(), &get(&url)?).map(Some)
}
