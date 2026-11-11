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
                        .hint_text(crate::i18n::text("interaction-palette-paste"))
                        .desired_width(ui.available_width() - 80.0)
                        .margin(egui::Margin::symmetric(8, 6)),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let valid = matches!(parsed, Some(Ok(_)));
                if ui
                    .add_enabled(
                        valid,
                        egui::Button::new(crate::i18n::text("interaction-palette-open-link")),
                    )
                    .clicked()
                    || (enter && valid)
                {
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
                crate::i18n::message(
                    "interaction-palette-clone-busy",
                    &[("url", url.as_str().into())],
                ),
                false,
            ));
            return;
        }
        self.start_clone();
        self.set_clone_url(&url);
        // The page chose it: worth a look before cloning.
        let non_ascii = !url.is_ascii();
        self.toasts.push(Toast::new(
            crate::i18n::message(
                "interaction-palette-clone-offer",
                &[
                    ("url", url.as_str().into()),
                    ("non_ascii", if non_ascii { "yes" } else { "no" }.into()),
                ],
            ),
            non_ascii,
        ));
    }
}

/// What a link opens, for the line under the field and the palette's entry.
pub(super) fn describe(link: &ForgeLink) -> String {
    let mut args = vec![
        ("repository", link.repo.as_str().into()),
        ("host", link.host.as_str().into()),
    ];
    let id = match &link.target {
        Target::Repo => "interaction-palette-target-repository",
        Target::PullRequest(number) => {
            args.push(("number", number.to_string().into()));
            "interaction-palette-target-pr"
        }
        Target::Commit(hash) => {
            args.push(("hash", hash.get(..8).unwrap_or(hash).into()));
            "interaction-palette-target-commit"
        }
        Target::File { path, line, .. } => {
            args.push(("path", path.as_str().into()));
            if let Some(line) = line {
                args.push(("line", line.to_string().into()));
                "interaction-palette-target-file-line"
            } else {
                "interaction-palette-target-file"
            }
        }
    };
    crate::i18n::message(id, &args)
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

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn whole_link_descriptions_localize_and_keep_refs_paths_urls_and_number_identity() {
        crate::i18n::set_language("de");
        let mut link = ForgeLink {
            http: false,
            host: "forge.example:3000".into(),
            repo: "team/日本語".into(),
            target: Target::PullRequest(1234),
            url: "https://forge.example:3000/team/日本語/pulls/1234".into(),
        };
        let raw = link.clone();
        assert_eq!(
            describe(&link),
            "team/日本語, Pull Request #1234 auf forge.example:3000"
        );
        assert_eq!(link, raw);
        link.target = Target::File {
            rev: "de/main".into(),
            path: "src/日本語.rs".into(),
            line: Some(1234),
        };
        assert_eq!(
            describe(&link),
            "team/日本語, src/日本語.rs in Zeile 1234 auf forge.example:3000"
        );
        assert!(matches!(&link.target, Target::File { rev, .. } if rev == "de/main"));
        crate::i18n::set_language("en-US");
        assert_eq!(
            describe(&link),
            "team/日本語, src/日本語.rs at line 1234 on forge.example:3000"
        );
        assert_eq!(link.url, raw.url);
    }
}
