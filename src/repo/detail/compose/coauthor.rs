//! _Add co-author_: the repository's authors (as `.mailmap` names them), filtered as you type;
//! the one picked goes into a `Co-authored-by:` trailer, written by `git interpret-trailers`.

use egui::{RichText, Ui};

use crate::git::{self, Author};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::Tokens;

/// The trailer GitHub, GitLab and others read co-authors from.
pub const TRAILER: &str = "Co-authored-by";

/// The most authors listed at once.
const MAX_LISTED: usize = 40;

/// The authors whose name or email has `filter` in it (any case), but not `me` (an email).
pub fn filtered<'a>(authors: &'a [Author], filter: &str, me: &str) -> Vec<&'a Author> {
    let filter = filter.trim().to_lowercase();
    authors
        .iter()
        .filter(|a| !a.email.eq_ignore_ascii_case(me.trim()))
        .filter(|a| {
            filter.is_empty()
                || a.name.to_lowercase().contains(&filter)
                || a.email.to_lowercase().contains(&filter)
        })
        .take(MAX_LISTED)
        .collect()
}

impl RepoTab {
    pub(super) fn coauthor_button(&mut self, ui: &mut Ui) {
        let b = icons::small_button(ui, "circle-user-round", crate::i18n::text("repo-core-0942"));
        if b.clicked() && !self.compose.authors_asked {
            self.load_authors(ui.ctx());
        }
        let t = Tokens::get(ui.ctx());
        let me = self.identity.email.clone();
        let busy = self.compose.adding_coauthor;
        let authors = &self.compose.authors;
        let filter = &mut self.compose.coauthor_filter;
        let mut picked: Option<Author> = None;
        egui::Popup::menu(&b)
            .width(300.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(filter)
                        .hint_text(crate::i18n::text("repo-core-0943"))
                        .desired_width(f32::INFINITY),
                );
                if b.clicked() {
                    field.request_focus();
                }
                match authors {
                    None => {
                        ui.add(crate::spinner::Spinner::new());
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(t.danger).small());
                    }
                    Some(Ok(all)) => {
                        let shown = filtered(all, filter, &me);
                        if shown.is_empty() {
                            ui.label(
                                RichText::new(crate::i18n::text("repo-core-0944"))
                                    .color(t.text_faint),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for a in shown {
                                    let text = format!("{} <{}>", a.name, a.email);
                                    let r =
                                        ui.add_enabled(!busy, egui::Button::new(text).truncate());
                                    let plural = if a.commits == 1 { "" } else { "s" };
                                    if r.on_hover_text(crate::i18n::message(
                                        "repo-core-0945",
                                        &[
                                            ("commits", format!("{}", a.commits).into()),
                                            ("plural", format!("{}", plural).into()),
                                        ],
                                    ))
                                    .clicked()
                                    {
                                        picked = Some(a.clone());
                                    }
                                }
                            });
                        let enter =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if enter && picked.is_none() {
                            picked = filtered(all, filter, &me).first().map(|a| (*a).clone());
                        }
                    }
                }
                if picked.is_some() {
                    ui.close();
                }
            });
        if let Some(author) = picked {
            self.compose.coauthor_filter.clear();
            self.add_coauthor(ui.ctx(), &author);
        }
    }

    fn load_authors(&mut self, ctx: &egui::Context) {
        self.compose.authors_asked = true;
        self.compose.authors = None;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || git::authors(&repo),
            |tab, found, _| tab.compose.authors = Some(found),
        );
    }

    /// Adds `author` as a co-author of the message, in the background; the box is left alone
    /// if it was changed meanwhile.
    fn add_coauthor(&mut self, ctx: &egui::Context, author: &Author) {
        self.compose.adding_coauthor = true;
        let before = super::join_message(&self.commit_summary, &self.commit_body);
        let repo = self.path.clone();
        let ident = author.ident();
        let message = before.clone();
        self.then(
            ctx,
            move || git::add_trailer(&repo, &message, TRAILER, &ident),
            move |tab, result, _| {
                tab.compose.adding_coauthor = false;
                match result {
                    Ok(message) => {
                        if super::join_message(&tab.commit_summary, &tab.commit_body) == before {
                            tab.set_message(&message);
                        } else {
                            tab.compose.notice =
                                Some(crate::i18n::text("repo-core-0946").to_owned());
                        }
                    }
                    Err(e) => tab.compose.notice = Some(e),
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn author(name: &str, email: &str) -> Author {
        Author {
            name: name.into(),
            email: email.into(),
            commits: 1,
        }
    }

    #[test]
    fn filters_by_name_or_email_but_not_me() {
        let all = [
            author("Ana Müller", "ana@x.org"),
            author("Bob", "bob@y.org"),
            author("Me", "Me@x.org"),
        ];
        let names = |filter: &str| -> Vec<String> {
            filtered(&all, filter, "me@x.org")
                .iter()
                .map(|a| a.name.clone())
                .collect()
        };
        assert_eq!(names(""), ["Ana Müller", "Bob"]);
        assert_eq!(names("MÜL"), ["Ana Müller"]);
        assert_eq!(names("y.org"), ["Bob"]);
        assert!(names("nobody").is_empty());
        let many: Vec<Author> = (0..100)
            .map(|i| author(&format!("a{i}"), &format!("{i}@x")))
            .collect();
        assert_eq!(filtered(&many, " ", "").len(), MAX_LISTED);
    }
}
