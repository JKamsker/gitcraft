//! Opt-in, bounded version-specific package metadata. No guessed changelog URLs.
mod archive;
#[cfg(test)]
mod tests;

use super::deps::{DepChange, Ecosystem, Registry};
use crate::{connections, network, tasks};
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Metadata {
    pub license: Option<String>,
    pub changelogs: BTreeMap<String, String>,
    pub archive_url: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    ecosystem: Ecosystem,
    old: Registry,
    new: Registry,
}
type Answer = Result<(Option<Metadata>, Option<Metadata>), String>;
struct Pending {
    rx: Receiver<Answer>,
    _scope: tasks::Scope,
    context: connections::Context,
}
#[derive(Default)]
pub(super) struct State {
    selected: Option<(String, Request)>,
    pending: Option<Pending>,
    cache: VecDeque<(Request, Answer)>,
    result: Option<Answer>,
}

pub(super) fn urls(
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
        return Err("Invalid registry package identity".into());
    }
    match ecosystem {
        Ecosystem::Cargo if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) =>
            Ok((None, format!("https://static.crates.io/crates/{name}/{name}-{version}.crate"))),
        Ecosystem::Npm if valid_npm(name) => Ok((Some(format!("https://registry.npmjs.org/{}/{version}", name.replace('/', "%2F"))), String::new())),
        _ => Err("Published licence/changelog lookup is available for crates.io and npm packages; this source is unavailable".into()),
    }
}
fn valid_npm(name: &str) -> bool {
    let parts: Vec<_> = name.trim_start_matches('@').split('/').collect();
    (parts.len() == 1 && !name.starts_with('@') || parts.len() == 2 && name.starts_with('@'))
        && parts.iter().all(|p| {
            !p.is_empty()
                && !matches!(*p, "." | "..")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
fn get(url: &str) -> Result<Vec<u8>, String> {
    if network::refused().is_some() {
        return Err("Offline mode blocks package metadata lookup".into());
    }
    if tasks::cancellation().is_cancelled() {
        return Err("cancelled".into());
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
        ureq::Error::Status(n, _) => format!("Package registry returned HTTP {n}"),
        _ => "Package registry request failed".into(),
    })?;
    if response.status() != 200 {
        return Err("Package registry redirected the request; refused".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((8 << 20) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read package metadata")?;
    if bytes.len() > 8 << 20 {
        return Err("Package download exceeds 8 MiB".into());
    }
    if tasks::cancellation().is_cancelled() {
        return Err("cancelled".into());
    }
    Ok(bytes)
}
fn parse(
    ecosystem: Ecosystem,
    name: &str,
    version: &str,
    archive_url: String,
    bytes: &[u8],
) -> Result<Metadata, String> {
    let mut files = archive::texts(bytes)?;
    let license = match ecosystem {
        Ecosystem::Cargo => {
            let manifest = files
                .remove("Cargo.toml")
                .ok_or("Package omits Cargo.toml")?;
            let doc = manifest
                .parse::<toml_edit::Document<String>>()
                .map_err(|_| "Invalid published Cargo.toml")?;
            let package = doc
                .get("package")
                .ok_or("Published manifest omits package")?;
            if package.get("name").and_then(|x| x.as_str()) != Some(name)
                || package.get("version").and_then(|x| x.as_str()) != Some(version)
            {
                return Err("Published package identity mismatch".into());
            }
            package
                .get("license")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        }
        Ecosystem::Npm => {
            let manifest: serde_json::Value = serde_json::from_str(
                &files
                    .remove("package.json")
                    .ok_or("Package omits package.json")?,
            )
            .map_err(|_| "Invalid published package.json")?;
            if manifest["name"].as_str() != Some(name)
                || manifest["version"].as_str() != Some(version)
            {
                return Err("Published package identity mismatch".into());
            }
            manifest["license"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        }
        _ => return Err("Unsupported package registry".into()),
    };
    files.remove("Cargo.toml");
    files.remove("package.json");
    Ok(Metadata {
        license,
        changelogs: files,
        archive_url,
    })
}
fn fetch(eco: Ecosystem, registry: &Registry) -> Result<Option<Metadata>, String> {
    let Some((name, version)) = registry else {
        return Ok(None);
    };
    let (manifest, mut url) = urls(eco, name, version)?;
    if let Some(endpoint) = manifest {
        let json: serde_json::Value =
            serde_json::from_slice(&get(&endpoint)?).map_err(|_| "Invalid registry metadata")?;
        if json["name"].as_str() != Some(name) || json["version"].as_str() != Some(version) {
            return Err("Registry package identity mismatch".into());
        }
        url = json
            .pointer("/dist/tarball")
            .and_then(|s| s.as_str())
            .ok_or("Registry omits published archive")?
            .to_owned();
        if !url.starts_with("https://registry.npmjs.org/") || url.contains(['?', '#', '\\']) {
            return Err("Registry archive points outside the public npm registry".into());
        }
    }
    parse(eco, name, version, url.clone(), &get(&url)?).map(Some)
}
impl State {
    pub fn select(&mut self, eco: Ecosystem, change: &DepChange, ctx: &egui::Context) {
        let request = Request {
            ecosystem: eco,
            old: change.metadata_registry.0.clone(),
            new: change.metadata_registry.1.clone(),
        };
        self.pending = None;
        self.selected = Some((change.name.clone(), request.clone()));
        self.result = self
            .cache
            .iter()
            .find(|(r, _)| *r == request)
            .map(|(_, a)| a.clone());
        if request.old.is_none() && request.new.is_none() {
            self.result = Some(Err("Metadata unavailable: the lockfile does not identify a supported public registry; no package name was sent".into()));
            return;
        }
        if self.result.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let scope = tasks::Scope::default();
        let repaint = ctx.clone();
        let work = request.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit(
                scope.0.clone(),
                tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let answer = (|| {
                        Ok((
                            fetch(work.ecosystem, &work.old)?,
                            fetch(work.ecosystem, &work.new)?,
                        ))
                    })();
                    let _ = tx.send(answer);
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submitted {
            Ok(()) => {
                self.pending = Some(Pending {
                    rx,
                    _scope: scope,
                    context: connections::current(),
                })
            }
            Err(e) => self.result = Some(Err(e)),
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.context != connections::current())
        {
            self.pending = None;
            self.result = Some(Err("Profile changed; select the package again".into()));
        }
        if let Some(pending) = &self.pending {
            match pending.rx.try_recv() {
                Ok(answer) => {
                    self.result = Some(answer.clone());
                    self.pending = None;
                    if let Some((_, request)) = &self.selected {
                        self.cache.push_back((request.clone(), answer));
                        if self.cache.len() > 8 {
                            self.cache.pop_front();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.result = Some(Err("Package metadata task cancelled or stopped".into()));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                }
            }
        }
        let Some((name, _)) = &self.selected else {
            return;
        };
        let name = name.clone();
        ui.horizontal(|ui| {
            ui.strong(format!("{name}: published licences and changelogs"));
            if ui.button("Close / cancel").clicked() {
                self.selected = None;
                self.pending = None;
                self.result = None;
            }
        });
        if self.pending.is_some() {
            ui.label("Reading the exact published versions (up to 12 seconds per request)…");
        }
        match &self.result {
            Some(Err(e)) => {
                ui.label(e);
            }
            Some(Ok((old, new))) => {
                let licence = |m: &Option<Metadata>| {
                    m.as_ref()
                        .map_or("not recorded as a supported public-registry package", |m| {
                            m.license
                                .as_deref()
                                .unwrap_or("not declared in published manifest")
                        })
                        .to_owned()
                };
                let changed = old
                    .as_ref()
                    .zip(new.as_ref())
                    .is_some_and(|(a, b)| a.license != b.license);
                ui.label(format!(
                    "Licence{}: {} → {}",
                    if changed { " changed" } else { "" },
                    licence(old),
                    licence(new)
                ));
                egui::ScrollArea::vertical().id_salt("dependency_metadata").max_height(240.0).show(ui, |ui| {
                    for (side,metadata) in [("Before",old),("After",new)] {
                        let Some(m)=metadata else { continue };
                        ui.hyperlink_to(format!("{side}: published archive"), &m.archive_url);
                        if m.changelogs.is_empty() { ui.label(format!("{side}: changelog unavailable — not included in the published package.")); }
                        for (path,text) in &m.changelogs { egui::CollapsingHeader::new(format!("{side} changelog: {path}")).id_salt((side,path)).show(ui, |ui| { ui.add(egui::Label::new(egui::RichText::new(text).monospace()).wrap()); }); }
                    }
                });
            }
            None => {}
        }
        ui.separator();
    }
}
