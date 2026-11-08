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
fn fetch(eco: Ecosystem, registry: &Registry) -> Result<Option<Metadata>, String> {
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
impl Metadata {
    fn license_summary(&self) -> String {
        let mut terms = Vec::new();
        if let Some(expression) = &self.license {
            terms.push(expression.clone());
        }
        if let Some((path, content)) = &self.license_file {
            terms.push(crate::i18n::message(
                "repo-core-3991",
                &[
                    ("path", format!("{}", path).into()),
                    (
                        "else",
                        format!(
                            "{}",
                            if content.is_err() {
                                crate::i18n::text("repo-core-3992")
                            } else {
                                ""
                            }
                        )
                        .into(),
                    ),
                ],
            ));
        }
        if let Some(note) = &self.license_note {
            terms.push(note.clone());
        }
        if terms.is_empty() {
            crate::i18n::text("repo-core-3993").into()
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
            &crate::i18n::message(
                "repo-core-3994",
                &[
                    ("side", format!("{}", side).into()),
                    ("archive-url", format!("{}", self.archive_url).into()),
                ],
            ),
            t.text_dim,
        );
        doc.text(
            &crate::i18n::message(
                "repo-core-3995",
                &[
                    ("side", format!("{}", side).into()),
                    (
                        "license-summary",
                        format!("{}", self.license_summary()).into(),
                    ),
                ],
            ),
            t.warning,
        );
        if let Some((path, content)) = &self.license_file {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3996",
                    &[
                        ("side", format!("{}", side).into()),
                        ("path", format!("{}", path).into()),
                    ],
                ),
                t.text,
            );
            match content {
                Ok(text) => doc.text(text, t.text),
                Err(error) => doc.text(error, t.warning),
            }
        }
        if self.changelogs.is_empty() {
            doc.text(
                &crate::i18n::message("repo-core-3997", &[("side", format!("{}", side).into())]),
                t.text_dim,
            );
        }
        for (path, text) in &self.changelogs {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3998",
                    &[
                        ("side", format!("{}", side).into()),
                        ("path", format!("{}", path).into()),
                    ],
                ),
                t.text,
            );
            doc.text(text, t.text);
        }
        for (path, reason) in &self.skipped {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3999",
                    &[
                        ("side", format!("{}", side).into()),
                        ("path", format!("{}", path).into()),
                        ("reason", format!("{}", reason).into()),
                    ],
                ),
                t.warning,
            );
        }
    }
}
fn side_summary(side: &Side) -> String {
    match side {
        Ok(Some(m)) => m.license_summary(),
        Ok(None) => crate::i18n::text("repo-core-4000").into(),
        Err(error) => {
            crate::i18n::message("repo-core-4001", &[("error", format!("{}", error).into())])
        }
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
            let error = crate::i18n::text("repo-core-4002").to_owned();
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
                Err(crate::i18n::text("repo-core-4003").into()),
                Err(crate::i18n::text("repo-core-4003").into()),
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
                        Err(crate::i18n::text("repo-core-4004").into()),
                        Err(crate::i18n::text("repo-core-4004").into()),
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
            &crate::i18n::message("repo-core-4005", &[("name", format!("{}", name).into())]),
            t.text,
        );
        if self.pending.is_some() {
            doc.text(crate::i18n::text("repo-core-4006"), t.text_dim);
        }
        if let Some(answer) = &self.result {
            doc.text(
                &crate::i18n::message(
                    "repo-core-4007",
                    &[
                        (
                            "else",
                            format!(
                                "{}",
                                if changed(answer) {
                                    crate::i18n::text("repo-core-4008")
                                } else {
                                    ""
                                }
                            )
                            .into(),
                        ),
                        ("answer", format!("{}", side_summary(&answer.0)).into()),
                        ("answer2", format!("{}", side_summary(&answer.1)).into()),
                    ],
                ),
                t.warning,
            );
            for (side, result) in [
                (crate::i18n::text("repo-core-1564"), &answer.0),
                (crate::i18n::text("repo-core-1565"), &answer.1),
            ] {
                match result {
                    Ok(Some(m)) => m.append_document(doc, side, t),
                    Ok(None) => doc.text(
                        &crate::i18n::message(
                            "repo-core-4009",
                            &[("side", format!("{}", side).into())],
                        ),
                        t.text_dim,
                    ),
                    Err(error) => doc.text(
                        &crate::i18n::message(
                            "repo-core-4010",
                            &[
                                ("side", format!("{}", side).into()),
                                ("error", format!("{}", error).into()),
                            ],
                        ),
                        t.warning,
                    ),
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
            ui.strong(crate::i18n::message(
                "repo-core-4005",
                &[("name", format!("{}", name).into())],
            ));
            if ui.button(crate::i18n::text("repo-core-4011")).clicked() {
                self.close();
            }
            if self.pending.is_none()
                && self
                    .result
                    .as_ref()
                    .is_some_and(|(a, b)| a.is_err() || b.is_err())
                && ui.button(crate::i18n::text("repo-core-4012")).clicked()
            {
                retry = true;
            }
        });
        if retry {
            self.start(name, request, ui.ctx(), true);
        }
        if self.pending.is_some() {
            ui.label(crate::i18n::text("repo-core-4013"));
        }
        if let Some(answer) = &self.result {
            ui.label(crate::i18n::message(
                "repo-core-4007",
                &[
                    (
                        "else",
                        format!(
                            "{}",
                            if changed(answer) {
                                crate::i18n::text("repo-core-4008")
                            } else {
                                ""
                            }
                        )
                        .into(),
                    ),
                    ("answer", format!("{}", side_summary(&answer.0)).into()),
                    ("answer2", format!("{}", side_summary(&answer.1)).into()),
                ],
            ));
            egui::ScrollArea::vertical()
                .id_salt("dependency_metadata")
                .max_height(240.0)
                .show(ui, |ui| {
                    for (side, result) in [
                        (crate::i18n::text("repo-core-1564"), &answer.0),
                        (crate::i18n::text("repo-core-1565"), &answer.1),
                    ] {
                        let metadata = match result {
                            Ok(Some(m)) => m,
                            Ok(None) => {
                                ui.label(crate::i18n::message(
                                    "repo-core-4009",
                                    &[("side", format!("{}", side).into())],
                                ));
                                continue;
                            }
                            Err(error) => {
                                ui.label(crate::i18n::message(
                                    "repo-core-4010",
                                    &[
                                        ("side", format!("{}", side).into()),
                                        ("error", format!("{}", error).into()),
                                    ],
                                ));
                                continue;
                            }
                        };
                        ui.hyperlink_to(
                            crate::i18n::message(
                                "repo-core-4015",
                                &[("side", format!("{}", side).into())],
                            ),
                            &metadata.archive_url,
                        );
                        if let Some((path, content)) = &metadata.license_file {
                            egui::CollapsingHeader::new(crate::i18n::message(
                                "repo-core-3996",
                                &[
                                    ("side", format!("{}", side).into()),
                                    ("path", format!("{}", path).into()),
                                ],
                            ))
                            .id_salt((side, "licence-file"))
                            .show(ui, |ui| match content {
                                Ok(text) => {
                                    ui.label(text);
                                }
                                Err(error) => {
                                    ui.label(error);
                                }
                            });
                        }
                        if metadata.changelogs.is_empty() {
                            ui.label(crate::i18n::message(
                                "repo-core-4017",
                                &[("side", format!("{}", side).into())],
                            ));
                        }
                        for (path, reason) in &metadata.skipped {
                            ui.label(crate::i18n::message(
                                "repo-core-3999",
                                &[
                                    ("side", format!("{}", side).into()),
                                    ("path", format!("{}", path).into()),
                                    ("reason", format!("{}", reason).into()),
                                ],
                            ));
                        }
                        for (path, text) in &metadata.changelogs {
                            egui::CollapsingHeader::new(crate::i18n::message(
                                "repo-core-3998",
                                &[
                                    ("side", format!("{}", side).into()),
                                    ("path", format!("{}", path).into()),
                                ],
                            ))
                            .id_salt((side, path))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(text).monospace()).wrap(),
                                );
                            });
                        }
                    }
                });
        }
        ui.separator();
    }
}
