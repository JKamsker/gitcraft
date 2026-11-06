//! What `cargo` and `npm` say a package holds, and where the published one is.

use std::path::Path;

use serde_json::Value;

use super::run::run;
use super::{Ecosystem, Package};

/// The packages of the repository at `repo` that can be published: Cargo's (`cargo metadata`)
/// and npm's (a `package.json` at the top that isn't private).
pub(crate) fn packages(repo: &Path, cargo: bool, npm: bool) -> Result<Vec<Package>, String> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if cargo {
        match run(
            repo,
            "cargo",
            &[
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--offline",
            ],
        ) {
            Ok(json) => match parse_cargo_metadata(&json, &own_folder(repo)) {
                Ok(found) => out.extend(found),
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(format!("cargo: {e}")),
        }
    }
    if npm {
        let text = crate::remote::read_to_string(&crate::remote::join(repo, "package.json"))
            .map_err(|e| e.to_string());
        match text {
            Ok(text) => out.extend(parse_package_json(&text)),
            Err(e) => errors.push(format!("package.json: {e}")),
        }
    }
    if out.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(out)
}

/// The repository's top as cargo spells it (the folder of its `Cargo.toml`), to find its
/// packages' folders in a workspace that may reach outside it (a submodule's superproject).
fn own_folder(repo: &Path) -> String {
    run(
        repo,
        "cargo",
        &["locate-project", "--message-format", "plain"],
    )
    .map(|p| {
        p.trim()
            .replace('\\', "/")
            .trim_end_matches("Cargo.toml")
            .trim_end_matches('/')
            .to_owned()
    })
    .unwrap_or_default()
}

/// The publishable packages of `cargo metadata --no-deps` in `root` (the repository's top as
/// cargo spells it; empty: the workspace's root), their folders relative to it.
pub(crate) fn parse_cargo_metadata(json: &str, root: &str) -> Result<Vec<Package>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("cargo metadata: {e}"))?;
    let root = if root.is_empty() {
        v.get("workspace_root")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .replace('\\', "/")
    } else {
        root.to_owned()
    };
    let mut out: Vec<Package> = v
        .get("packages")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                // `publish = false` reads as an empty list of registries.
                .filter(|p| {
                    p.get("publish")
                        .and_then(Value::as_array)
                        .is_none_or(|r| !r.is_empty())
                })
                .filter_map(|p| {
                    let manifest = p
                        .get("manifest_path")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .replace('\\', "/");
                    // Outside the repository: not one of its packages.
                    let inside = manifest.strip_prefix(&root)?;
                    if !inside.starts_with('/') {
                        return None;
                    }
                    let dir = inside
                        .trim_start_matches('/')
                        .trim_end_matches("Cargo.toml")
                        .trim_end_matches('/')
                        .to_owned();
                    // Only to other registries than crates.io.
                    let registry = p
                        .get("publish")
                        .and_then(Value::as_array)
                        .filter(|r| !r.iter().any(|x| x.as_str() == Some("crates-io")))
                        .map(|r| {
                            r.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        });
                    Some(Package {
                        ecosystem: Ecosystem::Cargo,
                        name: str_at(p, "name"),
                        version: str_at(p, "version"),
                        dir,
                        registry,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // The top package first, then the members by folder: as the workspace reads.
    out.sort_by(|a, b| (&a.dir, &a.name).cmp(&(&b.dir, &b.name)));
    Ok(out)
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The package of a top `package.json`, unless it is private (never published).
pub(crate) fn parse_package_json(text: &str) -> Option<Package> {
    let v: Value = serde_json::from_str(text).ok()?;
    if v.get("private").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let registry = v
        .pointer("/publishConfig/registry")
        .and_then(Value::as_str)
        .filter(|r| !r.contains("registry.npmjs.org"))
        .map(str::to_owned);
    Some(Package {
        ecosystem: Ecosystem::Npm,
        name: str_at(&v, "name"),
        version: str_at(&v, "version"),
        dir: String::new(),
        registry,
    })
}

/// The files `pkg` would ship, from its folder, and their sizes when the tool says them.
pub(crate) fn files(repo: &Path, pkg: &Package) -> Result<Vec<(String, Option<u64>)>, String> {
    let dir = if pkg.dir.is_empty() {
        repo.to_path_buf()
    } else {
        crate::remote::join(repo, &pkg.dir)
    };
    match pkg.ecosystem {
        Ecosystem::Cargo => {
            let out = run(
                &dir,
                "cargo",
                &["package", "--list", "--allow-dirty", "-p", &pkg.name],
            )?;
            Ok(out
                .lines()
                .map(|l| l.trim().replace('\\', "/"))
                .filter(|l| !l.is_empty())
                .map(|l| (l, None))
                .collect())
        }
        Ecosystem::Npm => {
            let out = run(
                &dir,
                "npm",
                &["pack", "--dry-run", "--json", "--ignore-scripts"],
            )?;
            parse_npm_pack(&out)
        }
    }
}

/// `npm pack --dry-run --json`: `[{"files": [{"path": "…", "size": 12}]}]`.
pub(crate) fn parse_npm_pack(json: &str) -> Result<Vec<(String, Option<u64>)>, String> {
    // npm may print notices before the JSON.
    let start = json.find('[').ok_or("npm said no package")?;
    let v: Value =
        serde_json::from_str(&json[start..]).map_err(|e| format!("npm pack's answer: {e}"))?;
    Ok(v.get(0)
        .and_then(|p| p.get("files"))
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .map(|f| (str_at(f, "path"), f.get("size").and_then(Value::as_u64)))
                .collect()
        })
        .unwrap_or_default())
}

/// The version published last (a release, not a pre-release, when there is one), and where
/// its archive is.
pub(crate) fn published(pkg: &Package) -> Result<Option<(String, String)>, String> {
    let web = match pkg.ecosystem {
        Ecosystem::Cargo => "https://crates.io",
        Ecosystem::Npm => "https://registry.npmjs.org",
    };
    let agent = crate::forge::http::with_proxy(ureq::AgentBuilder::new(), web)
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!(
            "gitcraft/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/Vancete/gitcraft)"
        ))
        .build();
    let get = |url: &str| -> Result<Option<Value>, String> {
        match agent.get(url).call() {
            Ok(r) => {
                let text = r.into_string().map_err(|e| e.to_string())?;
                serde_json::from_str(&text)
                    .map(Some)
                    .map_err(|e| format!("{url} didn't answer with JSON: {e}"))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(format!("couldn't ask {url}: {e}")),
        }
    };
    let name = &pkg.name;
    match pkg.ecosystem {
        Ecosystem::Cargo => {
            let Some(v) = get(&format!("https://crates.io/api/v1/crates/{name}"))? else {
                return Ok(None);
            };
            let version = v
                .pointer("/crate/max_stable_version")
                .and_then(Value::as_str)
                .or_else(|| v.pointer("/crate/newest_version").and_then(Value::as_str))
                .unwrap_or_default()
                .to_owned();
            // crates.io's own spelling: `proc_macro2` is `proc-macro2` there, and its
            // downloads go by it.
            let name = v
                .pointer("/crate/name")
                .and_then(Value::as_str)
                .unwrap_or(name);
            Ok((!version.is_empty()).then(|| {
                let url = format!("https://static.crates.io/crates/{name}/{name}-{version}.crate");
                (version, url)
            }))
        }
        Ecosystem::Npm => {
            let path = name.replace('/', "%2F");
            let Some(v) = get(&format!("https://registry.npmjs.org/{path}/latest"))? else {
                return Ok(None);
            };
            let version = str_at(&v, "version");
            let url = v
                .pointer("/dist/tarball")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok((!version.is_empty() && !url.is_empty()).then_some((version, url)))
        }
    }
}

/// The files of the archive at `url`.
pub(crate) fn published_files(url: &str) -> Result<Vec<String>, String> {
    let web: String = url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let agent = crate::forge::http::with_proxy(ureq::AgentBuilder::new(), &web)
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
        .build();
    let mut bytes = Vec::new();
    use std::io::Read;
    agent
        .get(url)
        .call()
        .map_err(|e| format!("couldn't download {url}: {e}"))?
        .into_reader()
        .take(super::archive::max() + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("couldn't download {url}: {e}"))?;
    if bytes.len() as u64 > super::archive::max() {
        return Err(super::archive::too_big());
    }
    super::archive::list(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_metadata_keeps_the_packages_that_can_be_published() {
        let json = r#"{"workspace_root": "C:\\work\\app", "packages": [
            {"name": "app", "version": "1.2.3", "manifest_path": "C:\\work\\app\\Cargo.toml", "publish": null},
            {"name": "tool", "version": "0.1.0", "manifest_path": "C:\\work\\app\\crates\\tool\\Cargo.toml", "publish": ["crates-io"]},
            {"name": "internal", "version": "0.1.0", "manifest_path": "C:\\work\\app\\internal\\Cargo.toml", "publish": []}
        ]}"#;
        let list = parse_cargo_metadata(json, "").unwrap_or_default();
        let names: Vec<(&str, &str)> = list
            .iter()
            .map(|p| (p.name.as_str(), p.dir.as_str()))
            .collect();
        assert_eq!(names, [("app", ""), ("tool", "crates/tool")]);
        assert!(list.iter().all(|p| p.registry.is_none()));
    }

    #[test]
    fn a_workspace_reaching_outside_the_repository_and_other_registries() {
        // The repository is `/w/sub`, a member of `/w`'s workspace.
        let json = r#"{"workspace_root": "/w", "packages": [
            {"name": "outer", "version": "1.0.0", "manifest_path": "/w/Cargo.toml", "publish": null},
            {"name": "sub", "version": "0.1.0", "manifest_path": "/w/sub/Cargo.toml", "publish": ["corp"]},
            {"name": "deep", "version": "0.1.0", "manifest_path": "/w/sub/deep/Cargo.toml", "publish": null},
            {"name": "sibling", "version": "0.1.0", "manifest_path": "/w/subway/Cargo.toml", "publish": null}
        ]}"#;
        let list = parse_cargo_metadata(json, "/w/sub").unwrap_or_default();
        let names: Vec<(&str, &str, Option<&str>)> = list
            .iter()
            .map(|p| (p.name.as_str(), p.dir.as_str(), p.registry.as_deref()))
            .collect();
        assert_eq!(names, [("sub", "", Some("corp")), ("deep", "deep", None)]);
        let npm = parse_package_json(
            r#"{"name": "@corp/x", "version": "1.0.0", "publishConfig": {"registry": "https://npm.corp.example/"}}"#,
        );
        assert_eq!(
            npm.and_then(|p| p.registry).as_deref(),
            Some("https://npm.corp.example/")
        );
    }

    #[test]
    fn package_json_and_npm_pack() {
        assert!(parse_package_json(r#"{"name":"x","private":true}"#).is_none());
        assert_eq!(
            parse_package_json(r#"{"name":"@a/x","version":"1.0.0"}"#).map(|p| p.name),
            Some("@a/x".to_owned())
        );
        let out =
            "npm notice\n[{\"files\":[{\"path\":\"index.js\",\"size\":12},{\"path\":\"ü.md\"}]}]";
        assert_eq!(
            parse_npm_pack(out).unwrap_or_default(),
            [("index.js".to_owned(), Some(12)), ("ü.md".to_owned(), None)]
        );
        assert!(parse_npm_pack("nothing").is_err());
    }
}
