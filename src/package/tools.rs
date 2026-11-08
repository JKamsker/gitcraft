//! What `cargo` and `npm` say a package holds, and where the published one is.

use std::path::Path;

use serde_json::Value;

use super::run::{run, run_witnessed};
use super::witness::{CargoFailure, Failure, PackageJsonFailure};
use super::{Ecosystem, Package};

/// The packages of the repository at `repo` that can be published: Cargo's (`cargo metadata`)
/// and npm's (a `package.json` at the top that isn't private).
pub(crate) fn packages(repo: &Path, cargo: bool, npm: bool) -> Result<Vec<Package>, String> {
    packages_witnessed(repo, cargo, npm).map_err(Failure::into_raw)
}

pub(super) fn packages_witnessed(
    repo: &Path,
    cargo: bool,
    npm: bool,
) -> Result<Vec<Package>, Failure> {
    let mut out = Vec::new();
    let mut cargo_error = None;
    let mut package_json_error = None;
    if cargo {
        match run_witnessed(
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
            Ok(json) => match parse_cargo_metadata_witnessed(&json, &own_folder(repo)) {
                Ok(found) => out.extend(found),
                Err(e) => cargo_error = Some(e),
            },
            Err(e) => cargo_error = Some(CargoFailure::tool(e)),
        }
    }
    if npm {
        let text = crate::remote::read_to_string(&crate::remote::join(repo, "package.json"))
            .map_err(|e| e.to_string());
        match text {
            Ok(text) => out.extend(parse_package_json(&text)),
            Err(e) => package_json_error = Some(PackageJsonFailure::new(e)),
        }
    }
    finish_packages(out, cargo_error, package_json_error)
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
    parse_cargo_metadata_witnessed(json, root).map_err(CargoFailure::into_raw)
}

pub(super) fn parse_cargo_metadata_witnessed(
    json: &str,
    root: &str,
) -> Result<Vec<Package>, CargoFailure> {
    let v: Value = serde_json::from_str(json).map_err(|e| {
        let detail = e.to_string();
        CargoFailure::Metadata {
            raw: format!("cargo metadata: {detail}"),
            detail,
        }
    })?;
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
    files_witnessed(repo, pkg).map_err(Failure::into_raw)
}

pub(super) fn files_witnessed(
    repo: &Path,
    pkg: &Package,
) -> Result<Vec<(String, Option<u64>)>, Failure> {
    let dir = if pkg.dir.is_empty() {
        repo.to_path_buf()
    } else {
        crate::remote::join(repo, &pkg.dir)
    };
    match pkg.ecosystem {
        Ecosystem::Cargo => {
            let out = run_witnessed(
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
            let out = run_witnessed(
                &dir,
                "npm",
                &["pack", "--dry-run", "--json", "--ignore-scripts"],
            )?;
            parse_npm_pack_witnessed(&out)
        }
    }
}

/// `npm pack --dry-run --json`: `[{"files": [{"path": "…", "size": 12}]}]`.
pub(crate) fn parse_npm_pack(json: &str) -> Result<Vec<(String, Option<u64>)>, String> {
    parse_npm_pack_witnessed(json).map_err(Failure::into_raw)
}

pub(super) fn parse_npm_pack_witnessed(json: &str) -> Result<Vec<(String, Option<u64>)>, Failure> {
    // npm may print notices before the JSON.
    let start = json.find('[').ok_or_else(|| Failure::NpmNoPackage {
        raw: "npm said no package".to_owned(),
    })?;
    let v: Value = serde_json::from_str(&json[start..]).map_err(|e| {
        let detail = e.to_string();
        Failure::NpmPackAnswer {
            raw: format!("npm pack's answer: {detail}"),
            detail,
        }
    })?;
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
    published_witnessed(pkg).map_err(Failure::into_raw)
}

pub(super) fn published_witnessed(pkg: &Package) -> Result<Option<(String, String)>, Failure> {
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
    let get = |url: &str| -> Result<Option<Value>, Failure> {
        match agent.get(url).call() {
            Ok(r) => {
                let text = r
                    .into_string()
                    .map_err(|e| Failure::Opaque(e.to_string()))?;
                parse_registry_json(url, &text)
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(ask_registry_failure(url, e.to_string())),
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
    published_files_witnessed(url).map_err(Failure::into_raw)
}

pub(super) fn published_files_witnessed(url: &str) -> Result<Vec<String>, Failure> {
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
        .map_err(|e| download_failure(url, e.to_string()))?
        .into_reader()
        .take(super::archive::max() + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| download_failure(url, e.to_string()))?;
    super::archive::check_size(bytes.len() as u64)?;
    super::archive::list_witnessed(&bytes)
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

// The real aggregation point: preserve cargo → package.json order and original raw join.
pub(super) fn finish_packages(
    out: Vec<Package>,
    cargo: Option<CargoFailure>,
    package_json: Option<PackageJsonFailure>,
) -> Result<Vec<Package>, Failure> {
    if out.is_empty() && (cargo.is_some() || package_json.is_some()) {
        let errors: Vec<&str> = cargo
            .as_ref()
            .map(CargoFailure::raw)
            .into_iter()
            .chain(package_json.as_ref().map(PackageJsonFailure::raw))
            .collect();
        let raw = errors.join("; ");
        return Err(Failure::Packages {
            raw,
            cargo: cargo.map(Box::new),
            package_json,
        });
    }
    Ok(out)
}

fn registry_json_failure(url: &str, detail: String) -> Failure {
    Failure::RegistryJson {
        raw: format!("{url} didn't answer with JSON: {detail}"),
        url: url.to_owned(),
        detail,
    }
}
fn ask_registry_failure(url: &str, detail: String) -> Failure {
    Failure::AskRegistry {
        raw: format!("couldn't ask {url}: {detail}"),
        url: url.to_owned(),
        detail,
    }
}
fn download_failure(url: &str, detail: String) -> Failure {
    Failure::Download {
        raw: format!("couldn't download {url}: {detail}"),
        url: url.to_owned(),
        detail,
    }
}

#[cfg(test)]
#[path = "tools/witness_tests.rs"]
mod witness_tests;

fn parse_registry_json(url: &str, text: &str) -> Result<Option<Value>, Failure> {
    serde_json::from_str(text)
        .map(Some)
        .map_err(|e| registry_json_failure(url, e.to_string()))
}
