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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Metadata {
    pub license: Option<String>,
    pub changelogs: BTreeMap<String, String>,
    pub archive_url: String,
    pub license_file: Option<(String, Result<String, String>)>,
    pub license_note: Option<String>,
    pub skipped: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    ecosystem: Ecosystem,
    old: Registry,
    new: Registry,
}
type Side = Result<Option<Metadata>, String>;
type Answer = (Side, Side);
fn read_both(
    eco: Ecosystem,
    old: &Registry,
    new: &Registry,
    mut read: impl FnMut(Ecosystem, &Registry) -> Side,
) -> Answer {
    // A missing/broken predecessor must never prevent reading the updated release.
    (read(eco, old), read(eco, new))
}
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
    generation: u64,
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
                    .unwrap_or_else(|| "Package omits a regular Cargo.toml".into())
            })?;
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
                    selected.files.remove(&normal).ok_or_else(|| selected.skipped.remove(&normal)
                        .unwrap_or_else(|| "declared licence file is missing, not a regular file, or unavailable in the archive".into()))
                } else {
                    Err("declared licence file path is unsafe or outside the package".into())
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
                        .unwrap_or_else(|| "Package omits a regular package.json".into())
                })?)
                .map_err(|_| "Invalid published package.json")?;
            if manifest["name"].as_str() != Some(name)
                || manifest["version"].as_str() != Some(version)
            {
                return Err("Published package identity mismatch".into());
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
                license_note = Some("licence declaration uses an unsupported representation; its meaning is unknown".into());
            }
            license
        }
        _ => return Err("Unsupported package registry".into()),
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
impl Metadata {
    fn license_summary(&self) -> String {
        let mut terms = Vec::new();
        if let Some(expression) = &self.license {
            terms.push(expression.clone());
        }
        if let Some((path, content)) = &self.license_file {
            terms.push(format!(
                "declared file {path}{}",
                if content.is_err() {
                    " (content unavailable)"
                } else {
                    ""
                }
            ));
        }
        if let Some(note) = &self.license_note {
            terms.push(note.clone());
        }
        if terms.is_empty() {
            "not declared in published manifest".into()
        } else {
            terms.join("; ")
        }
    }
    fn license_complete(&self) -> bool {
        self.license_note.is_none()
            && self
                .license_file
                .as_ref()
                .is_none_or(|(_, text)| text.is_ok())
    }
    fn licence_eq(&self, other: &Self) -> bool {
        self.license == other.license && self.license_file == other.license_file
    }
    fn append_document(
        &self,
        doc: &mut crate::repo::find::rendered::Document,
        side: &str,
        t: &crate::theme::Tokens,
    ) {
        doc.text(
            &format!("{side}: published archive · {}", self.archive_url),
            t.text_dim,
        );
        doc.text(
            &format!("{side} licence: {}", self.license_summary()),
            t.warning,
        );
        if let Some((path, content)) = &self.license_file {
            doc.text(&format!("{side} declared licence file: {path}"), t.text);
            match content {
                Ok(text) => doc.text(text, t.text),
                Err(error) => doc.text(error, t.warning),
            }
        }
        if self.changelogs.is_empty() {
            doc.text(&format!("{side}: no readable recognised changelog in retained metadata; other filenames may exist."), t.text_dim);
        }
        for (path, text) in &self.changelogs {
            doc.text(&format!("{side} changelog: {path}"), t.text);
            doc.text(text, t.text);
        }
        for (path, reason) in &self.skipped {
            doc.text(&format!("{side}: {path} · {reason}"), t.warning);
        }
    }
}
fn side_summary(side: &Side) -> String {
    match side {
        Ok(Some(m)) => m.license_summary(),
        Ok(None) => "not recorded as a supported public-registry package".into(),
        Err(error) => format!("unknown ({error})"),
    }
}
fn changed(answer: &Answer) -> bool {
    match answer {
        (Ok(Some(a)), Ok(Some(b))) if a.license_complete() && b.license_complete() => {
            !a.licence_eq(b)
        }
        _ => false,
    }
}
impl State {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn select(&mut self, eco: Ecosystem, change: &DepChange, ctx: &egui::Context) {
        self.start(
            change.name.clone(),
            Request {
                ecosystem: eco,
                old: change.metadata_registry.0.clone(),
                new: change.metadata_registry.1.clone(),
            },
            ctx,
            false,
        );
    }
    fn start(&mut self, name: String, request: Request, ctx: &egui::Context, retry: bool) {
        self.start_using(name, request, ctx, retry, fetch);
    }
    fn start_using(
        &mut self,
        name: String,
        request: Request,
        ctx: &egui::Context,
        retry: bool,
        mut read: impl FnMut(Ecosystem, &Registry) -> Side + Send + 'static,
    ) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.selected = Some((name, request.clone()));
        if retry {
            self.cache.retain(|(r, _)| r != &request);
        }
        self.result = self
            .cache
            .iter()
            .find(|(r, _)| *r == request)
            .map(|(_, a)| a.clone());
        if request.old.is_none() && request.new.is_none() {
            let error = "Metadata unavailable: the lockfile does not identify a supported public registry; no package name was sent".to_owned();
            self.result = Some((Err(error.clone()), Err(error)));
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
                    let answer = read_both(work.ecosystem, &work.old, &work.new, &mut read);
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
            Err(e) => self.result = Some((Err(e.clone()), Err(e))),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.context != connections::current())
        {
            self.pending = None;
            self.result = Some((
                Err("Profile changed; select the package again".into()),
                Err("Profile changed; select the package again".into()),
            ));
            self.generation = self.generation.wrapping_add(1);
        }
        if let Some(pending) = &self.pending {
            match pending.rx.try_recv() {
                Ok(answer) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.pending = None;
                    // Offline, cancelled and transient responses remain retryable. Partial
                    // successes stay visible but never poison the complete-result cache.
                    if answer.0.is_ok()
                        && answer.1.is_ok()
                        && let Some((_, request)) = &self.selected
                    {
                        self.cache.retain(|(r, _)| r != request);
                        self.cache.push_back((request.clone(), answer.clone()));
                        if self.cache.len() > 8 {
                            self.cache.pop_front();
                        }
                    }
                    self.result = Some(answer);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.generation = self.generation.wrapping_add(1);
                    self.result = Some((
                        Err("Package metadata task cancelled or stopped".into()),
                        Err("Package metadata task cancelled or stopped".into()),
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
    }
    fn close(&mut self) {
        self.selected = None;
        self.pending = None;
        self.result = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn append_document(
        &self,
        doc: &mut crate::repo::find::rendered::Document,
        t: &crate::theme::Tokens,
    ) {
        let Some((name, _)) = &self.selected else {
            return;
        };
        doc.text(
            &format!("{name}: published licences and changelogs"),
            t.text,
        );
        if self.pending.is_some() {
            doc.text("Reading the exact published versions…", t.text_dim);
        }
        if let Some(answer) = &self.result {
            doc.text(
                &format!(
                    "Licence{}: {} → {}",
                    if changed(answer) { " changed" } else { "" },
                    side_summary(&answer.0),
                    side_summary(&answer.1)
                ),
                t.warning,
            );
            for (side, result) in [("Before", &answer.0), ("After", &answer.1)] {
                match result {
                    Ok(Some(m)) => m.append_document(doc, side, t),
                    Ok(None) => doc.text(
                        &format!("{side}: metadata unavailable for this source"),
                        t.text_dim,
                    ),
                    Err(error) => doc.text(&format!("{side}: {error}"), t.warning),
                }
            }
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        let Some((name, request)) = self.selected.clone() else {
            return;
        };
        let mut retry = false;
        ui.horizontal(|ui| {
            ui.strong(format!("{name}: published licences and changelogs"));
            if ui.button("Close / cancel").clicked() {
                self.close();
            }
            if self.pending.is_none()
                && self
                    .result
                    .as_ref()
                    .is_some_and(|(a, b)| a.is_err() || b.is_err())
                && ui.button("Retry metadata").clicked()
            {
                retry = true;
            }
        });
        if retry {
            self.start(name, request, ui.ctx(), true);
        }
        if self.pending.is_some() {
            ui.label("Reading the exact published versions (up to 12 seconds per request)…");
        }
        if let Some(answer) = &self.result {
            ui.label(format!(
                "Licence{}: {} → {}",
                if changed(answer) { " changed" } else { "" },
                side_summary(&answer.0),
                side_summary(&answer.1)
            ));
            egui::ScrollArea::vertical().id_salt("dependency_metadata").max_height(240.0).show(ui,|ui| {
                for (side,result) in [("Before",&answer.0),("After",&answer.1)] {
                    let metadata = match result {
                        Ok(Some(m))=>m,
                        Ok(None)=>{ui.label(format!("{side}: metadata unavailable for this source"));continue;},
                        Err(error)=>{ui.label(format!("{side}: {error}"));continue;},
                    };
                    ui.hyperlink_to(format!("{side}: published archive"),&metadata.archive_url);
                    if let Some((path,content)) = &metadata.license_file {
                        egui::CollapsingHeader::new(format!("{side} declared licence file: {path}")).id_salt((side,"licence-file")).show(ui,|ui| {
                            match content {Ok(text)=>{ui.label(text);},Err(error)=>{ui.label(error);}}
                        });
                    }
                    if metadata.changelogs.is_empty() {ui.label(format!("{side}: changelog unavailable — no readable recognised filename in retained metadata; other filenames may exist."));}
                    for (path,reason) in &metadata.skipped {ui.label(format!("{side}: {path} · {reason}"));}
                    for (path,text) in &metadata.changelogs {
                        egui::CollapsingHeader::new(format!("{side} changelog: {path}")).id_salt((side,path)).show(ui,|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(text).monospace()).wrap());
                        });
                    }
                }
            });
        }
        ui.separator();
    }
}
