//! What the tab shows of the owners: the tag on a file's row and the line above a list of files.

use egui::{RichText, Ui};

use super::Summary;
use crate::codeowners;
use crate::repo::RepoTab;
use crate::repo::widgets::OwnerTag;
use crate::theme::Tokens;

/// How long an owner's name may be on a row before it is cut.
const TAG_CHARS: usize = 18;

/// The names of `owners` as a summary shows them: a team by its name, unless two teams of
/// different organisations share it.
fn short_names(owners: &[String]) -> Vec<String> {
    owners
        .iter()
        .map(|o| {
            let short = codeowners::short_owner(o);
            let shared = owners
                .iter()
                .filter(|other| codeowners::short_owner(other).eq_ignore_ascii_case(short))
                .count()
                > 1;
            if shared {
                o.trim_start_matches('@').to_owned()
            } else {
                short.to_owned()
            }
        })
        .collect()
}

/// "Needs payments and infra · 2 files without an owner", for the change of a list's files.
pub(crate) fn summary_text(owners: &[String], unowned: usize, files: usize) -> Option<String> {
    if owners.is_empty() && unowned == 0 {
        return None;
    }
    let mut text = if owners.is_empty() {
        String::new()
    } else {
        format!("Needs {}", codeowners::and_list(&short_names(owners)))
    };
    if unowned > 0 {
        if !text.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(&if unowned == files {
            if files == 1 {
                "No owner".to_owned()
            } else {
                "No file here has an owner".to_owned()
            }
        } else if unowned == 1 {
            "1 file without an owner".to_owned()
        } else {
            format!("{unowned} files without an owner")
        });
    }
    Some(text)
}

fn rules(n: usize) -> String {
    if n == 1 {
        "1 rule".to_owned()
    } else {
        format!("{n} rules")
    }
}

/// `name`, cut to [`TAG_CHARS`] with an ellipsis.
fn cut(name: &str) -> String {
    if name.chars().count() <= TAG_CHARS {
        name.to_owned()
    } else {
        let mut cut: String = name.chars().take(TAG_CHARS - 1).collect();
        cut.push('…');
        cut
    }
}

impl RepoTab {
    /// What a row of the file at `path` shows of its owners.
    pub(in crate::repo) fn owner_tag(&self, path: &str) -> Option<OwnerTag> {
        let owners = self.owners.of(path);
        let first = owners.first()?;
        let name = cut(codeowners::short_owner(first));
        let label = match owners.len() {
            1 => name,
            n => format!("{name} +{}", n - 1),
        };
        Some(OwnerTag {
            label,
            full: owners.join(", "),
        })
    }

    /// The line above a list of files that says who they need (once worked out).
    pub(in crate::repo) fn owners_summary(&self, ui: &mut Ui, paths: &[&str]) {
        let Some(summary) = self.owners_summary_of(paths, ui.ctx()) else {
            return;
        };
        let Summary {
            owners,
            unowned,
            files,
        } = summary.as_ref();
        let Some(text) = summary_text(owners, *unowned, *files) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let (path, count) = self
            .owners
            .file()
            .map(|f| (f.path.clone(), f.rule_count()))
            .unwrap_or_default();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 4))
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).color(t.text_dim).small()).truncate())
                    .on_hover_ui(|ui| {
                        ui.label(format!("From {path} at HEAD ({})", rules(count)));
                        if !owners.is_empty() {
                            ui.label(owners.join(", "));
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teams_of_the_same_name_keep_their_organisation() {
        let owners = [
            "@a/payments".to_owned(),
            "@b/payments".to_owned(),
            "@ann".to_owned(),
        ];
        assert_eq!(short_names(&owners), ["a/payments", "b/payments", "ann"]);
    }

    #[test]
    fn long_names_are_cut() {
        assert_eq!(cut("short"), "short");
        assert_eq!(
            cut("a-very-long-team-name-indeed").chars().count(),
            TAG_CHARS
        );
        assert!(cut("a-very-long-team-name-indeed").ends_with('…'));
    }
}
