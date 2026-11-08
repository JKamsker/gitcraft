//! A folder browser for another machine (WSL, SSH): its folders listed by a script run there,
//! in the background, so a slow connection never holds the window up. Git repositories are
//! marked; a click picks a folder, a double click opens it.

use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Sense, Ui, vec2};

use crate::icons;
use crate::remote::{self, Host};
use crate::theme::Tokens;

/// The most folders listed of one folder (a few thousand rows are still quick to draw).
const MOST: usize = 5000;

/// A folder of the one listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    /// Holds a repository (`.git` in it).
    pub repo: bool,
}

/// What a folder holds, as listed there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The folder, as an absolute path there (`~` and links resolved by `cd`).
    pub dir: String,
    /// The folder itself holds a repository.
    pub repo: bool,
    pub folders: Vec<Folder>,
    /// More than [`MOST`] folders: the rest left out.
    pub cut: bool,
}

/// What came of the browser this frame.
pub enum Outcome {
    /// This folder (a path there) was picked.
    Picked(String),
    Cancelled,
}

pub struct FolderBrowser {
    host: Host,
    /// The folder shown, or being listed.
    listing: Option<Listing>,
    /// The folder asked for, while it is listed; what came of it.
    loading: Option<(String, Receiver<Result<Listing, String>>)>,
    /// Original requested directory and raw failure; neither is translated.
    error: Option<(String, String)>,
    /// The folder of the listing clicked, by its index.
    selected: Option<usize>,
    hidden: bool,
}

impl FolderBrowser {
    /// A browser of `host`'s folders, listing `start` (a path there; `~` when empty or not one).
    pub fn new(host: Host, start: &str, ctx: &egui::Context) -> Self {
        let start = start.trim();
        let start = if start.starts_with('/') || start == "~" || start.starts_with("~/") {
            start
        } else {
            "~"
        };
        let mut browser = Self {
            host,
            listing: None,
            loading: None,
            error: None,
            selected: None,
            hidden: false,
        };
        browser.go(start.to_owned(), ctx);
        browser
    }

    /// Lists `dir` in the background; the folder shown stays until it comes.
    fn go(&mut self, dir: String, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let host = self.host.clone();
        let ctx = ctx.clone();
        let asked = dir.clone();
        std::thread::spawn(move || {
            let _ = tx.send(list(&host, &asked));
            ctx.request_repaint();
        });
        self.loading = Some((dir, rx));
        self.error = None;
    }

    /// The listing asked for, once it came.
    fn take_listing(&mut self) {
        let Some((asked, rx)) = &self.loading else {
            return;
        };
        let Ok(result) = rx.try_recv() else { return };
        let asked = asked.clone();
        self.loading = None;
        match result {
            Ok(listing) => {
                self.listing = Some(listing);
                self.selected = None;
            }
            Err(e) => self.error = Some((asked, e)),
        }
    }

    /// The folder "Select" picks: the one clicked, else the one shown.
    fn choice(&self) -> Option<String> {
        let listing = self.listing.as_ref()?;
        Some(match self.selected.and_then(|i| listing.folders.get(i)) {
            Some(folder) => child(&listing.dir, &folder.name),
            None => listing.dir.clone(),
        })
    }

    /// Draws the browser: where it is, the folders there, Select and Cancel.
    pub fn ui(&mut self, ui: &mut Ui) -> Option<Outcome> {
        self.take_listing();
        let t = Tokens::get(ui.ctx());
        let mut go_to = None;
        let mut outcome = None;
        self.bar(ui, &t, &mut go_to);
        ui.add_space(6.0);
        if let Some((dir, e)) = &self.error {
            ui.colored_label(t.danger, display_list_error(e, dir, &self.host));
            ui.add_space(4.0);
        }
        let frame = egui::Frame::new()
            .fill(t.field)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_sm)
            .inner_margin(egui::Margin::same(4));
        frame.show(ui, |ui| {
            ui.set_min_height(280.0);
            ui.set_width(ui.available_width());
            self.rows(ui, &t, &mut go_to, &mut outcome);
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let choice = self.choice();
            let label = match (&choice, self.selected) {
                (Some(_), Some(_)) => crate::i18n::text("interaction-browser-select"),
                _ => crate::i18n::text("interaction-browser-select-current"),
            };
            if crate::repo::primary_button(ui, label, choice.is_some(), 140.0).clicked()
                && let Some(choice) = choice
            {
                outcome = Some(Outcome::Picked(choice));
            }
            if ui
                .add(
                    egui::Button::new(crate::i18n::text("interaction-browser-back"))
                        .min_size(vec2(80.0, 30.0)),
                )
                .clicked()
            {
                outcome = Some(Outcome::Cancelled);
            }
            ui.checkbox(
                &mut self.hidden,
                crate::i18n::text("interaction-browser-hidden"),
            );
        });
        if let Some(dir) = go_to {
            self.go(dir, ui.ctx());
        }
        outcome
    }

    /// Up, home, the folder's path (each part a link), reload.
    fn bar(&mut self, ui: &mut Ui, t: &Tokens, go_to: &mut Option<String>) {
        let dir = self.listing.as_ref().map(|l| l.dir.clone());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let up = dir.as_deref().filter(|d| *d != "/");
            if ui
                .add_enabled_ui(up.is_some(), |ui| {
                    icons::small_button(ui, "arrow-up", crate::i18n::text("interaction-browser-up"))
                })
                .inner
                .clicked()
                && let Some(d) = up
            {
                *go_to = Some(remote::parent_there(d).to_owned());
            }
            if icons::small_button(
                ui,
                "user-round",
                crate::i18n::text("interaction-browser-home"),
            )
            .clicked()
            {
                *go_to = Some("~".to_owned());
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("interaction-browser-reload"),
            )
            .clicked()
                && let Some(d) = &dir
            {
                *go_to = Some(d.clone());
            }
            ui.add_space(4.0);
            match &dir {
                Some(d) => {
                    for (n, (label, path)) in crumbs(d).into_iter().enumerate() {
                        // `/ home / me`: the root, then a slash between the others.
                        if n > 1 {
                            ui.label(RichText::new("/").color(t.text_faint));
                        }
                        if ui
                            .add(
                                egui::Label::new(RichText::new(label).color(t.text_dim))
                                    .sense(Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            *go_to = Some(path);
                        }
                    }
                }
                None => {
                    ui.label(RichText::new(self.host.label()).color(t.text_dim));
                }
            }
            if self.loading.is_some() {
                ui.add(crate::spinner::Spinner::new());
            }
        });
    }

    /// The folders, a repository's marked.
    fn rows(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        go_to: &mut Option<String>,
        outcome: &mut Option<Outcome>,
    ) {
        let Some(listing) = &self.listing else {
            if self.loading.is_some() {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "interaction-browser-connecting",
                        &[("host", self.host.label().into())],
                    ))
                    .color(t.text_faint),
                );
            }
            return;
        };
        if listing.repo {
            ui.horizontal(|ui| {
                icons::inline(ui, "folder-git-2", 14.0, t.accent_fg);
                ui.label(
                    RichText::new(crate::i18n::text("interaction-browser-is-repo"))
                        .color(t.accent_fg)
                        .small(),
                );
            });
        }
        let shown: Vec<(usize, &Folder)> = listing
            .folders
            .iter()
            .enumerate()
            .filter(|(_, f)| self.hidden || !f.name.starts_with('.'))
            .collect();
        if shown.is_empty() {
            ui.label(
                RichText::new(crate::i18n::text("interaction-browser-empty"))
                    .color(t.text_faint)
                    .small(),
            );
        }
        let row_h = 24.0;
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_h, shown.len(), |ui, range| {
                for &(i, folder) in &shown[range] {
                    let selected = self.selected == Some(i);
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                    crate::repo::widgets::a11y_selected(
                        &resp,
                        || {
                            crate::i18n::message(
                                if folder.repo {
                                    "interaction-browser-repo-label"
                                } else {
                                    "interaction-browser-folder-label"
                                },
                                &[("name", folder.name.as_str().into())],
                            )
                        },
                        selected,
                    );
                    if selected || resp.hovered() {
                        let fill = if selected { t.row_selected } else { t.hover };
                        ui.painter().rect_filled(rect, t.radius_sm, fill);
                    }
                    let (icon, tint) = if folder.repo {
                        ("folder-git-2", t.accent_fg)
                    } else {
                        ("folder-open", t.text_faint)
                    };
                    let icon_rect = egui::Rect::from_min_size(
                        rect.left_center() + vec2(6.0, -7.0),
                        vec2(14.0, 14.0),
                    );
                    icons::paint(ui, icon_rect, icon, 14.0, tint);
                    ui.painter().text(
                        rect.left_center() + vec2(26.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &folder.name,
                        egui::FontId::proportional(13.0),
                        t.text,
                    );
                    if folder.repo {
                        ui.painter().text(
                            rect.right_center() - vec2(8.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            crate::i18n::text("interaction-browser-repository"),
                            egui::FontId::proportional(11.0),
                            t.accent_fg,
                        );
                    }
                    if resp.double_clicked() {
                        *go_to = Some(child(&listing.dir, &folder.name));
                    } else if resp.clicked() {
                        self.selected = Some(i);
                    }
                }
            });
        if listing.cut {
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-browser-limit",
                    &[("max", MOST.into())],
                ))
                .color(t.text_faint)
                .small(),
            );
        }
        // A repository double-clicked is picked, rather than opened as a folder.
        if let Some(dir) = go_to.as_ref()
            && let Some(f) = listing
                .folders
                .iter()
                .find(|f| f.repo && child(&listing.dir, &f.name) == *dir)
        {
            *outcome = Some(Outcome::Picked(child(&listing.dir, &f.name)));
            *go_to = None;
        }
    }
}

/// Presentation from the original request values, never by splitting opaque paths or host names.
fn display_list_error(raw: &str, dir: &str, host: &Host) -> String {
    let host = host.label();
    let args = [("dir", dir.into()), ("host", host.as_str().into())];
    if raw == format!("There is no folder {dir} on {host}.") {
        return crate::i18n::message("interaction-browser-missing", &args);
    }
    if raw == format!("Could not list {dir} on {host}.") {
        return crate::i18n::message("interaction-browser-list-invalid", &args);
    }
    if let Some(detail) = raw.strip_prefix(&format!("Could not list {dir} on {host}: ")) {
        return crate::i18n::message(
            "interaction-browser-list-failed",
            &[
                ("dir", dir.into()),
                ("host", host.as_str().into()),
                ("detail", detail.into()),
            ],
        );
    }
    let remote = crate::remote::display::diagnostic(raw);
    if remote == raw {
        crate::git::display_error(raw)
    } else {
        remote
    }
}

/// The script listing `dir`'s folders: the folder (as `cd` found it), whether it holds a
/// repository, then each folder, `R` or `D` before its name; NUL after each (a name can hold a
/// newline). `M` at the end: there were more.
fn script(dir: &str) -> String {
    format!(
        concat!(
            "cd -- {dir} || exit 3\n",
            "printf '%s\\0' \"$(pwd)\"\n",
            "if [ -e .git ]; then printf 'R\\0'; else printf 'D\\0'; fi\n",
            "n=0\n",
            "for d in * .[!.]* ..?*; do\n",
            "  [ -d \"$d\" ] || continue\n",
            "  n=$((n+1))\n",
            "  if [ \"$n\" -gt {most} ]; then printf 'M\\0'; break; fi\n",
            "  if [ -e \"$d/.git\" ]; then printf 'R%s\\0' \"$d\"; else printf 'D%s\\0' \"$d\"; fi\n",
            "done\n",
        ),
        dir = remote::sh_path(dir),
        most = MOST,
    )
}

/// Lists `dir` on `host` (blocking: from a thread of its own).
fn list(host: &Host, dir: &str) -> Result<Listing, String> {
    let out = remote::exec(
        host,
        &remote::Request {
            script: script(dir),
            ..remote::Request::default()
        },
    )?;
    if out.code == 3 {
        return Err(format!("There is no folder {dir} on {}.", host.label()));
    }
    if out.code != 0 {
        return Err(format!(
            "Could not list {dir} on {}: {}",
            host.label(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse(&out.stdout).ok_or_else(|| format!("Could not list {dir} on {}.", host.label()))
}

/// What [`script`] printed.
fn parse(out: &[u8]) -> Option<Listing> {
    let mut parts = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    let dir = parts.next()?.into_owned();
    if !dir.starts_with('/') {
        return None;
    }
    let repo = parts.next()? == "R";
    let mut folders = Vec::new();
    let mut cut = false;
    for part in parts {
        let mut chars = part.chars();
        match chars.next() {
            Some('M') => cut = true,
            Some(kind @ ('R' | 'D')) => folders.push(Folder {
                name: chars.as_str().to_owned(),
                repo: kind == 'R',
            }),
            _ => {}
        }
    }
    folders.sort_by_key(|f| (f.name.starts_with('.'), f.name.to_lowercase()));
    Some(Listing {
        dir,
        repo,
        folders,
        cut,
    })
}

/// `name` in `dir`, both there.
fn child(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The parts of `dir` to click on: (what is shown, the folder it opens).
fn crumbs(dir: &str) -> Vec<(String, String)> {
    let mut list = vec![("/".to_owned(), "/".to_owned())];
    let mut path = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        path.push('/');
        path.push_str(part);
        list.push((part.to_owned(), path.clone()));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_reads_folders_and_repositories() {
        let out = b"/home/me\0D\0Dsrc\0Rapp\0D.cache\0Dodd\nname\0";
        let listing = parse(out);
        assert_eq!(
            listing,
            Some(Listing {
                dir: "/home/me".to_owned(),
                repo: false,
                folders: vec![
                    Folder {
                        name: "app".to_owned(),
                        repo: true
                    },
                    Folder {
                        name: "odd\nname".to_owned(),
                        repo: false
                    },
                    Folder {
                        name: "src".to_owned(),
                        repo: false
                    },
                    // Hidden ones last.
                    Folder {
                        name: ".cache".to_owned(),
                        repo: false
                    },
                ],
                cut: false,
            })
        );
        let cut = parse(b"/\0R\0Dx\0M\0");
        assert_eq!(cut.as_ref().map(|l| (l.repo, l.cut)), Some((true, true)));
        // Not what the script prints.
        assert_eq!(parse(b"oops"), None);
        assert_eq!(parse(b""), None);
    }

    #[test]
    fn paths_and_their_parts() {
        assert_eq!(child("/", "home"), "/home");
        assert_eq!(child("/home/me", "a b"), "/home/me/a b");
        assert_eq!(
            crumbs("/home/me"),
            vec![
                ("/".to_owned(), "/".to_owned()),
                ("home".to_owned(), "/home".to_owned()),
                ("me".to_owned(), "/home/me".to_owned()),
            ]
        );
        assert_eq!(crumbs("/"), vec![("/".to_owned(), "/".to_owned())]);
    }

    /// The machines `GITCRAFT_TEST_WSL` and `GITCRAFT_TEST_SSH` name (see `remote::live_tests`),
    /// with the repository folder named for SSH.
    fn test_hosts() -> Vec<(Host, Option<String>)> {
        let mut hosts = Vec::new();
        if let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") {
            hosts.push((Host::Wsl(distro), None));
        }
        if let Ok(ssh) = std::env::var("GITCRAFT_TEST_SSH")
            && let Some((host, repo)) = ssh.split_once(':')
        {
            hosts.push((Host::Ssh(host.to_owned()), Some(repo.to_owned())));
        }
        hosts
    }

    /// Against a real machine (skipped without one): odd names, hidden folders, a repository
    /// among them, a folder that isn't there.
    #[test]
    fn folders_there_are_listed() -> Result<(), String> {
        for (host, repo) in test_hosts() {
            let scratch = format!("/tmp/gitcraft-browse-{}", std::process::id());
            let made = remote::exec(
                &host,
                &remote::Request {
                    script: format!(
                        concat!(
                            "rm -rf {s} && mkdir -p {s}/'a b' {s}/.hidden {s}/repo/.git ",
                            "{s}/\"$(printf 'new\\nline')\" && : > {s}/file"
                        ),
                        s = scratch
                    ),
                    ..remote::Request::default()
                },
            )?;
            assert_eq!(made.code, 0, "{}", String::from_utf8_lossy(&made.stderr));
            let listed = list(&host, &scratch);
            let _ = remote::exec(
                &host,
                &remote::Request {
                    script: format!("rm -rf {scratch}"),
                    ..remote::Request::default()
                },
            );
            let listed = listed?;
            let names: Vec<(&str, bool)> = listed
                .folders
                .iter()
                .map(|f| (f.name.as_str(), f.repo))
                .collect();
            assert_eq!(
                names,
                vec![
                    ("a b", false),
                    ("new\nline", false),
                    ("repo", true),
                    (".hidden", false)
                ],
                "{host:?}"
            );
            assert_eq!(listed.dir, scratch);
            assert!(!listed.repo);

            let home = list(&host, "~")?;
            assert!(home.dir.starts_with('/') && home.dir != "/", "{home:?}");
            assert!(list(&host, "/no/such/folder/here").is_err());
            if let Some(repo) = repo {
                let here = list(&host, &repo)?;
                assert!(here.repo, "{here:?}");
                let name = here.dir.rsplit('/').next().unwrap_or_default();
                let parent = list(&host, remote::parent_there(&here.dir))?;
                assert!(
                    parent.folders.iter().any(|f| f.name == name && f.repo),
                    "{parent:?}"
                );
            }
        }
        Ok(())
    }

    /// The folder is quoted for the shell, `~` left to be the home folder.
    #[test]
    fn the_script_names_the_folder_safely() {
        assert!(script("/x/it's $(id)").starts_with(r"cd -- '/x/it'\''s $(id)' || exit 3"));
        assert!(script("~/repos").starts_with("cd -- \"$HOME\"'/repos' || exit 3"));
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_inert_browser_picks_original_path_and_keeps_raw_recorded_listing_failures()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let browser = FolderBrowser {
            host: Host::Ssh("recorded-host".into()),
            listing: Some(Listing {
                dir: "/opaque/root".into(),
                repo: true,
                cut: true,
                folders: vec![
                    Folder {
                        name: "日本語".into(),
                        repo: true,
                    },
                    Folder {
                        name: ".hidden".into(),
                        repo: false,
                    },
                ],
            }),
            loading: None,
            error: None,
            selected: None,
            hidden: false,
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (FolderBrowser, Option<String>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                if let Some(Outcome::Picked(path)) = state.0.ui(ui) {
                    state.1 = Some(path);
                }
            },
            (browser, None),
        );
        h.run();
        h.get_by_label("Dieser Ordner ist ein Git-Repository.");
        h.get_by_label("Nur die ersten 5.000 Ordner werden aufgelistet.");
        h.get_by_label("日本語, Repository").click();
        h.run();
        assert_eq!(h.state().0.selected, Some(0));
        h.get_by_label("Ordner auswählen").click();
        h.run();
        assert_eq!(h.state().1.as_deref(), Some("/opaque/root/日本語"));
        assert!(h.state().0.loading.is_none());
        assert_eq!(h.state().0.host, Host::Ssh("recorded-host".into()));
        h.get_by_label("Versteckte Ordner").click();
        h.run();
        assert!(h.state().0.hidden);
        h.get_by_label(".hidden, Ordner");
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("Select folder");
        h.get_by_label(".hidden, folder");
        assert_eq!(h.state().0.selected, Some(0));
        // Empty supplied receiver drives actual pending paint and completion routing without a worker.
        crate::i18n::set_language("de");
        let (tx, rx) = channel();
        let dir = "/opaque/name on SSH · recorded-host: literal".to_owned();
        h.state_mut().0.listing = None;
        h.state_mut().0.loading = Some((dir.clone(), rx));
        h.run_steps(3);
        h.get_by_label("Verbindung zu SSH · recorded-host wird hergestellt…");
        let raw = format!("There is no folder {dir} on SSH · recorded-host.");
        tx.send(Err(raw.clone())).map_err(|e| e.to_string())?;
        h.run();
        h.get_by_label(&format!(
            "Auf SSH · recorded-host gibt es keinen Ordner {dir}."
        ));
        assert_eq!(h.state().0.error, Some((dir.clone(), raw.clone())));
        let native_raw = "it is a symbolic link".to_owned();
        h.state_mut().0.error = Some((dir.clone(), native_raw.clone()));
        h.run();
        h.get_by_label("Dies ist ein symbolischer Link.");
        assert_eq!(h.state().0.error, Some((dir.clone(), native_raw.clone())));
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label(&native_raw);
        assert_eq!(h.state().0.error, Some((dir, native_raw)));
        Ok(())
    }

    #[test]
    fn known_listing_envelopes_use_original_typed_directory_and_host_and_keep_unknown_details_opaque()
     {
        crate::i18n::set_language("de");
        let host = Host::Ssh("recorded-host".into());
        let dir = "/日本語 on SSH · recorded-host: literal";
        let detail = "external 日本語: Could not list anything\nunchanged";
        let raw = format!("Could not list {dir} on {}: {detail}", host.label());
        assert_eq!(
            display_list_error(&raw, dir, &host),
            format!(
                "Der Ordner {dir} auf SSH · recorded-host konnte nicht aufgelistet werden: {detail}"
            )
        );
        let malformed = format!("Could not list {dir} on {}.", host.label());
        assert_eq!(
            display_list_error(&malformed, dir, &host),
            format!("Der Ordner {dir} auf SSH · recorded-host konnte nicht aufgelistet werden.")
        );
        let unknown = "Could not list another opaque external diagnostic";
        assert_eq!(display_list_error(unknown, dir, &host), unknown);
        assert_eq!(
            display_list_error(&raw, "different original request", &host),
            raw
        );
        crate::i18n::set_language("en-US");
        assert_eq!(display_list_error(&raw, dir, &host), raw);
        assert_eq!(display_list_error(&malformed, dir, &host), malformed);
    }
}
