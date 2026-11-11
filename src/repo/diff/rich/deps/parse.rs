use super::*;

pub fn packages(file: &str, text: &str) -> Result<Vec<Pkg>, String> {
    match file {
        "Cargo.lock" | "poetry.lock" | "uv.lock" => Ok(toml_packages(&toml::parse(text)?)),
        "package-lock.json" | "npm-shrinkwrap.json" => Ok(npm_packages(&json::parse(text)?)),
        "pnpm-lock.yaml" => Ok(pnpm_packages(&yaml::parse(text)?)),
        "composer.lock" => Ok(composer_packages(&json::parse(text)?)),
        "go.sum" => Ok(go_packages(text)),
        _ => Err(crate::i18n::text("repo-core-2109").to_owned()),
    }
}

fn get<'a>(node: &'a Node, key: &str) -> Option<&'a Field> {
    match node {
        Node::Map(fields) => fields.iter().find(|f| f.key == key),
        _ => None,
    }
}

fn text<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match &get(node, key)?.node {
        Node::Scalar { text, .. } => Some(text),
        _ => None,
    }
}

/// A licence as written: a string, or a list of them (Composer), or `{type: …}` (old npm).
fn license(node: &Node, key: &str) -> Option<String> {
    match &get(node, key)?.node {
        Node::Scalar { text, .. } => Some(text.clone()),
        Node::List(items) => Some(
            items
                .iter()
                .filter_map(|i| match &i.node {
                    Node::Scalar { text, .. } => Some(text.as_str()),
                    other => text(other, "type"),
                })
                .collect::<Vec<_>>()
                .join(" OR "),
        ),
        other => text(other, "type").map(str::to_owned),
    }
}

/// `[[package]]` tables with `name` and `version` (Cargo, Poetry, uv).
fn toml_packages(root: &Node) -> Vec<Pkg> {
    let Some(Field {
        node: Node::List(items),
        ..
    }) = get(root, "package")
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|i| {
            let (name, version) = (text(&i.node, "name")?, text(&i.node, "version")?);
            // Cargo: a git source pins a commit (shown, so a bump of it is seen); no source
            // at all is a workspace member or path dependency. uv: an editable or virtual
            // source is the project itself.
            let source = text(&i.node, "source");
            let git = source
                .filter(|s| s.starts_with("git+"))
                .and_then(|s| s.rsplit_once('#'))
                .map(|(_, rev)| rev.get(..7).unwrap_or(rev));
            let shown = match git {
                Some(rev) => format!("{version} (git {rev})"),
                None => version.to_owned(),
            };
            let pkg = Pkg::new(name, &shown, license(&i.node, "license"), i.line);
            let uv_local = matches!(
                get(&i.node, "source").map(|f| &f.node),
                Some(Node::Map(m)) if m.iter().any(|f| matches!(f.key.as_str(), "editable" | "virtual" | "directory" | "path"))
            );
            let cargo_local = source.is_none() && get(&i.node, "checksum").is_none() && get(&i.node, "source").is_none();
            let pkg = if git.is_some() || uv_local || (cargo_local && !is_poetry(root)) || source.is_some_and(|s| s.starts_with("registry+") && s != "registry+https://github.com/rust-lang/crates.io-index" && s != "registry+sparse+https://index.crates.io/") {
                pkg.local()
            } else {
                pkg
            };
            Some(Pkg {
                public_registry: matches!(source, Some("registry+https://github.com/rust-lang/crates.io-index" | "registry+sparse+https://index.crates.io/")),
                registry: pkg.registry.map(|(n, _)| (n, version.to_owned())),
                ..pkg
            })
        })
        .collect()
}

/// Whether a `[[package]]` lockfile is Poetry's, whose packages name no source.
fn is_poetry(root: &Node) -> bool {
    get(root, "metadata").is_some_and(|m| get(&m.node, "content-hash").is_some())
}

/// npm's lockfile: `packages` keyed by their folder (v2, v3), or nested `dependencies` (v1).
fn npm_packages(root: &Node) -> Vec<Pkg> {
    if let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(root, "packages")
    {
        return fields
            .iter()
            .filter(|f| !f.key.is_empty())
            .filter_map(|f| {
                // `node_modules/a/node_modules/@b/c` is `@b/c`.
                let name = f
                    .key
                    .rsplit_once("node_modules/")
                    .map_or(f.key.as_str(), |(_, n)| n);
                let name = text(&f.node, "name").unwrap_or(name);
                let mut pkg = Pkg::new(
                    name,
                    text(&f.node, "version")?,
                    license(&f.node, "license"),
                    f.line,
                );
                pkg.public_registry = text(&f.node, "resolved")
                    .is_some_and(|r| r.starts_with("https://registry.npmjs.org/"));
                // A workspace (not under `node_modules`) or a link is the project's own.
                let local = !f.key.contains("node_modules/")
                    || text(&f.node, "link") == Some("true")
                    || text(&f.node, "resolved")
                        .is_some_and(|r| !r.starts_with("https://registry.npmjs.org/"));
                Some(if local { pkg.local() } else { pkg })
            })
            .collect();
    }
    let mut out = Vec::new();
    npm_v1(root, &mut out, 0);
    out
}

fn npm_v1(node: &Node, out: &mut Vec<Pkg>, depth: usize) {
    let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(node, "dependencies")
    else {
        return;
    };
    if depth > 64 {
        return;
    }
    for f in fields {
        if let Some(version) = text(&f.node, "version") {
            let mut pkg = Pkg::new(&f.key, version, None, f.line);
            pkg.public_registry = text(&f.node, "resolved")
                .is_some_and(|r| r.starts_with("https://registry.npmjs.org/"));
            // An alias, `npm:other@1.2.0`: the registry has `other`.
            if let Some((real, v)) = version.strip_prefix("npm:").and_then(|a| {
                a.rfind('@')
                    .filter(|&at| at > 0)
                    .map(|at| (&a[..at], &a[at + 1..]))
            }) {
                pkg.registry = Some((real.to_owned(), v.to_owned()));
            }
            if version.starts_with("file:") || version.starts_with("link:") {
                pkg = pkg.local();
            }
            out.push(pkg);
        }
        npm_v1(&f.node, out, depth + 1);
    }
}

/// pnpm's lockfile: `packages` keyed `name@version` (v9), `/name@version` or `/name/version`.
fn pnpm_packages(root: &Node) -> Vec<Pkg> {
    let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(root, "packages")
    else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|f| {
            let key = f.key.trim_start_matches('/');
            // `(react@18.2.0)` after the version names the peers it was resolved with.
            let key = key.split_once('(').map_or(key, |(k, _)| k);
            // pnpm 5 and older: `name/1.0.0`, peers after `_` (`name/1.0.0_react@18.2.0`).
            let old_style = key
                .rsplit_once('/')
                .filter(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()))
                .or_else(|| {
                    let (k, _) = key.split_once('_')?;
                    k.rsplit_once('/')
                        .filter(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()))
                });
            // Newer: the last `@` past the first character, `@scope/name@1.0.0`.
            let (name, version) = match old_style {
                Some((name, version)) => (name, version.split('_').next().unwrap_or(version)),
                None => {
                    let at = key.rfind('@').filter(|&at| at > 0)?;
                    (&key[..at], &key[at + 1..])
                }
            };
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(Pkg::new(name, version, None, f.line))
        })
        .collect()
}

/// Composer's lockfile: `packages` and `packages-dev`, lists of `name` / `version`.
fn composer_packages(root: &Node) -> Vec<Pkg> {
    let mut out = Vec::new();
    for key in ["packages", "packages-dev"] {
        let Some(Field {
            node: Node::List(items),
            ..
        }) = get(root, key)
        else {
            continue;
        };
        out.extend(items.iter().filter_map(|i| {
            let (name, version) = (text(&i.node, "name")?, text(&i.node, "version")?);
            // A branch (`dev-main`) is pinned by its commit: shown, so a bump of it is seen.
            let reference = get(&i.node, "dist")
                .or_else(|| get(&i.node, "source"))
                .and_then(|d| text(&d.node, "reference"))
                .filter(|_| version.starts_with("dev-"));
            let shown = match reference {
                Some(r) => format!("{version} ({})", r.get(..7).unwrap_or(r)),
                None => version.to_owned(),
            };
            let mut pkg = Pkg::new(name, &shown, license(&i.node, "license"), i.line);
            pkg.registry = Some((name.to_owned(), version.to_owned()));
            Some(pkg)
        }));
    }
    out
}

/// `go.sum`: `module version hash` lines. It keeps the `/go.mod` lines of versions no longer
/// used, so each module is read as its newest version there.
fn go_packages(text: &str) -> Vec<Pkg> {
    let mut newest: Vec<Pkg> = Vec::new();
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        let version = version.trim_end_matches("/go.mod");
        let pkg = Pkg::new(name, version, None, n as u32 + 1);
        match at.get(name) {
            Some(&k) if version_cmp(version, &newest[k].version).is_gt() => newest[k] = pkg,
            Some(_) => {}
            None => {
                at.insert(name.to_owned(), newest.len());
                newest.push(pkg);
            }
        }
    }
    newest
}
