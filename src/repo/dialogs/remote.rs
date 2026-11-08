//! Dialogs about a remote: add or edit one, sign in to one, open a pull request on one.

use egui::{RichText, Ui};

use crate::repo::{Action, Refresh};
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title, usable};

/// Adds a remote and fetches it, or renames one and changes its URL.
pub(super) fn remote(
    ui: &mut Ui,
    enter: bool,
    original: &Option<(String, String)>,
    name: &mut String,
    url: &mut String,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let heading = if original.is_some() {
        crate::i18n::text("repo-core-1462")
    } else {
        crate::i18n::text("repo-core-1463")
    };
    title(
        ui,
        heading,
        crate::i18n::text("repo-core-1464"),
        &t,
    );
    field(ui, name, "origin", focus);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(url)
            .hint_text("https://github.com/user/repo.git")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.add_space(8.0);
    let valid = usable(name) && usable(url);
    let label = if original.is_some() { crate::i18n::text("repo-core-1062") } else { crate::i18n::text("repo-core-0976") };
    buttons(ui, label, valid, enter, || {
        let (name, url) = (name.trim().to_owned(), url.trim().to_owned());
        let steps = match original {
            None => vec![
                vec!["remote".into(), "add".into(), name.clone(), url],
                vec!["fetch".into(), name.clone()],
            ],
            Some((old_name, old_url)) => {
                let mut steps = Vec::new();
                if *old_name != name {
                    steps.push(vec![
                        "remote".into(),
                        "rename".into(),
                        old_name.clone(),
                        name.clone(),
                    ]);
                }
                if *old_url != url {
                    steps.push(vec!["remote".into(), "set-url".into(), name.clone(), url]);
                }
                steps
            }
        };
        Action::Ops {
            label: crate::i18n::message("repo-core-1468", &[("name", format!("{}", name).into())]),
            ops: steps,
            refresh: Refresh::All,
        }
    })
}

/// Asks for the username and password of `url`, after `refused` ones if they were.
#[allow(clippy::too_many_arguments)]
pub(super) fn credentials(
    ui: &mut Ui,
    enter: bool,
    url: &mut String,
    endpoints: &[String],
    refused: bool,
    username: &mut String,
    password: &mut String,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let host = host_of(url);
    title(
        ui,
        &crate::i18n::message("repo-core-1469", &[("host", format!("{}", host).into())]),
        crate::i18n::text("repo-core-1470"),
        &t,
    );
    if url.is_empty() || endpoints.len() > 1 {
        ui.label(crate::i18n::text("repo-core-1471"));
        egui::ComboBox::from_id_salt("https-failed-endpoint")
            .selected_text(if url.is_empty() {
                crate::i18n::text("repo-core-1473")
            } else {
                url.as_str()
            })
            .show_ui(ui, |ui| {
                for endpoint in endpoints {
                    ui.selectable_value(url, endpoint.clone(), endpoint);
                }
            });
    } else {
        ui.label(crate::i18n::message("repo-core-1474", &[("url", format!("{}", url).into())]));
    }
    if refused {
        ui.colored_label(
            t.danger,
            crate::i18n::text("repo-core-1475"),
        );
        ui.add_space(6.0);
        if crate::settings::network_page::valid_https(url)
            && ui.button(crate::i18n::text("repo-core-1476")).clicked()
        {
            return Outcome::Done(Box::new(Action::SwitchHttps(url.to_owned())));
        }
    }
    field(ui, username, crate::i18n::text("repo-core-1477"), focus);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(password)
            .password(true)
            .hint_text(crate::i18n::text("repo-core-1478"))
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            crate::i18n::text("repo-core-1479"),
        )
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(8.0);
    let scope = crate::git::CredentialScope::for_url(url, username.trim()).ok();
    let valid = scope.as_ref().is_some_and(|scope| scope.is_https())
        && !password.is_empty()
        && !password.chars().any(char::is_control);
    if !scope.as_ref().is_some_and(|scope| scope.is_https()) {
        ui.label(crate::i18n::text("repo-core-1480"));
    }
    buttons(ui, crate::i18n::text("repo-core-1481"), valid, enter, || {
        Action::SignIn(crate::git::Credentials {
            scope: scope.clone(),
            username: username.trim().to_owned(),
            password: password.clone(),
        })
    })
}

/// A pull request of `branch` into `base`, once the branch is pushed.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_pr(
    ui: &mut Ui,
    noun: &str,
    branch: &str,
    base: &mut String,
    pr_title: &mut String,
    body: &mut String,
    draft: &mut bool,
    reviewers: &mut [crate::repo::owners::Suggested],
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        &crate::i18n::message("repo-core-1482", &[("noun", format!("{}", noun).into())]),
        &crate::i18n::message("repo-core-1483", &[("branch", format!("{}", branch).into()), ("base", format!("{}", base).into())]),
        &t,
    );
    field(ui, pr_title, crate::i18n::text("repo-core-1484"), focus);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(body)
            .hint_text(crate::i18n::text("repo-core-1485"))
            .desired_width(f32::INFINITY)
            .desired_rows(5)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1486")).color(t.text_dim));
        ui.add(egui::TextEdit::singleline(base).desired_width(160.0));
        ui.checkbox(draft, crate::i18n::text("repo-core-1487"));
    });
    crate::repo::owners::reviewers_ui(ui, reviewers, *draft);
    ui.add_space(8.0);
    let valid = !pr_title.trim().is_empty() && !base.trim().is_empty();
    buttons(ui, crate::i18n::text("repo-core-1216"), valid, false, || Action::CreatePr {
        branch: branch.to_owned(),
        base: base.trim().to_owned(),
        title: pr_title.trim().to_owned(),
        body: body.trim().to_owned(),
        draft: *draft,
        reviewers: if *draft {
            Vec::new()
        } else {
            crate::repo::owners::asked(reviewers)
        },
    })
}

/// `github.com` from `https://user@github.com/owner/repo.git`.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    // The user info ends at the last `@` before the path: one in the path isn't it.
    let authority = rest.split('/').next().unwrap_or(rest);
    let rest = authority.rsplit_once('@').map_or(authority, |(_, r)| r);
    rest.split(['/', ':'])
        .next()
        .filter(|h| !h.is_empty())
        .unwrap_or(crate::i18n::text("repo-core-1488"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://user@github.com/owner/repo.git"),
            "github.com"
        );
        assert_eq!(
            host_of("https://gitlab.example.com:8443/a/b"),
            "gitlab.example.com"
        );
        assert_eq!(host_of("git@github.com:owner/repo.git"), "github.com");
        // An `@` in the path is not user info.
        assert_eq!(
            host_of("https://good.example/u@evil.example/r"),
            "good.example"
        );
        assert_eq!(host_of(""), "the remote");
    }
}
