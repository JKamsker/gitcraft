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
        // Said why it isn't one only once it starts like a link.
        let parsed = looks_like_link(&typed).then(|| link::parse_any(&typed));
        let mut open = false;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
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
        });
        // Kept in the field: if it can't be opened (not fetched yet…), it is there to try again.
        if open && let Some(Ok(link)) = parsed {
            self.open_link(link, &ui.ctx().clone());
        }
    }

    /// Opens the clone dialog for a linked repository no clone of which is here (one already
    /// open keeps what is typed in it).
    pub(super) fn offer_clone(&mut self, url: String) {
        use super::panels::Toast;
        if self.clone_form.is_some() || self.cloning.is_some() {
            self.toasts.push(Toast::new(
                format!(
                    "No clone of {url} among your repositories: once the clone under way is \
                     done, clone it and open the link again"
                ),
                false,
            ));
            return;
        }
        self.start_clone();
        self.set_clone_url(&url);
        // The page chose it: worth a look before cloning.
        let odd = if url.is_ascii() {
            ""
        } else {
            " Its name has letters outside ASCII: check it is the one you meant."
        };
        self.toasts.push(Toast::new(
            format!(
                "No clone of {url} among your repositories: clone it below, then open the link \
                 again.{odd}"
            ),
            !odd.is_empty(),
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

/// Whether what is typed starts as a link does (and so is read as one, or refused as one).
fn looks_like_link(text: &str) -> bool {
    ["gitcraft:", "https://", "http://"].iter().any(|p| {
        text.get(..p.len())
            .is_some_and(|h| h.eq_ignore_ascii_case(p))
    })
}

/// The link typed into the palette, or why it isn't one, when what is typed starts as one.
pub(super) fn palette_link(query: &str) -> Option<Result<ForgeLink, String>> {
    let q = query.trim();
    looks_like_link(q).then(|| link::parse_any(q))
}
