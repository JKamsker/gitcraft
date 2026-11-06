//! Links to a forge's pages pasted into the start page or the palette: opened as a
//! `gitcraft://open?url=…` link from the browser is (see `crate::launch::link`).

use egui::{RichText, Ui};

use super::App;
use crate::launch::Command;
use crate::launch::link::{self, ForgeLink, Target};
use crate::theme::Tokens;

impl App {
    /// Opens `link` in its clone (or offers to clone it).
    pub(super) fn open_link(&mut self, link: ForgeLink, ctx: &egui::Context) {
        let cwd = std::env::current_dir().unwrap_or_default();
        self.launch(Command::Link(link), cwd, ctx);
    }

    /// The start page's field for a pasted link.
    pub(super) fn home_link_field(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.add_space(16.0);
        let typed = self.home_link.trim().to_owned();
        let parsed = (!typed.is_empty()).then(|| link::parse_any(&typed));
        let mut open = false;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.home_link)
                    .hint_text("Paste a link to a pull request, commit or file on a forge…")
                    .desired_width(ui.available_width() - 80.0)
                    .margin(egui::Margin::symmetric(8, 6)),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let valid = matches!(parsed, Some(Ok(_)));
            if ui.add_enabled(valid, egui::Button::new("Open")).clicked() || (enter && valid) {
                open = true;
            }
        });
        match &parsed {
            Some(Ok(link)) => {
                ui.label(RichText::new(describe(link)).color(t.text_dim).small());
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(t.text_faint).small());
            }
            None => {}
        }
        if open && let Some(Ok(link)) = parsed {
            self.home_link.clear();
            self.open_link(link, &ui.ctx().clone());
        }
    }

    /// Opens the clone dialog for a linked repository no clone of which is here.
    pub(super) fn offer_clone(&mut self, url: String) {
        self.start_clone();
        self.set_clone_url(&url);
        self.toasts.push(super::panels::Toast::new(
            format!("No clone of {url} among your repositories: clone it?"),
            false,
        ));
    }
}

/// What a link opens, for the line under the field and the palette's entry.
pub(super) fn describe(link: &ForgeLink) -> String {
    let what = match &link.target {
        Target::Repo => String::new(),
        Target::PullRequest(n) => format!(", pull request #{n}"),
        Target::Commit(sha) => format!(", commit {}", sha.get(..8).unwrap_or(sha)),
        Target::File { path, line, .. } => match line {
            Some(n) => format!(", {path} at line {n}"),
            None => format!(", {path}"),
        },
    };
    format!("{}{what} on {}", link.repo, link.host)
}

/// The link typed into the palette, when what is typed is one.
pub(super) fn palette_link(query: &str) -> Option<ForgeLink> {
    let q = query.trim();
    let looks = ["gitcraft:", "https://", "http://"]
        .iter()
        .any(|p| q.get(..p.len()).is_some_and(|h| h.eq_ignore_ascii_case(p)));
    looks.then(|| link::parse_any(q).ok()).flatten()
}
