//! Commit signing: the format, key and program git signs with and whether it signs commits and
//! tags by default, read from and written to the user's global git config.

use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use super::layout::{self, Card, Filter, section};
use crate::theme::Tokens;

/// How long values read from the config hold while nothing is being edited: it can change
/// outside gitcraft.
pub(super) const RECHECK: Duration = Duration::from_secs(30);

/// `gpg.format` values, with what the settings page calls them.
const FORMATS: [(&str, &str); 3] = [
    ("openpgp", "OpenPGP (GPG)"),
    ("ssh", "SSH"),
    ("x509", "X.509 (S/MIME)"),
];

/// The signing part of a git config, as it is set there (or as a profile sets it).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Signing {
    /// `gpg.format`; empty when unset (git then uses openpgp).
    pub format: String,
    /// `user.signingkey`.
    pub key: String,
    /// The program for the format: `gpg.program`, `gpg.ssh.program` or `gpg.x509.program`.
    pub program: String,
    /// `commit.gpgSign`.
    pub sign_commits: bool,
    /// `tag.gpgSign`.
    pub sign_tags: bool,
}

impl Signing {
    /// The format git signs with, `openpgp` when none is set.
    pub(super) fn format(&self) -> &str {
        if self.format.is_empty() {
            "openpgp"
        } else {
            &self.format
        }
    }
}

/// The config key of the signing program for `format`.
pub(super) fn program_key(format: &str) -> &'static str {
    program_keys(format)[0]
}

/// Every config key git reads the signing program for `format` from, the one gitcraft writes
/// first. OpenPGP's is `gpg.program` or its newer name `gpg.openpgp.program`, whichever comes
/// last.
fn program_keys(format: &str) -> &'static [&'static str] {
    match format {
        "ssh" => &["gpg.ssh.program"],
        "x509" => &["gpg.x509.program"],
        _ => &["gpg.program", "gpg.openpgp.program"],
    }
}

/// The signing settings in `config --list -z` output, entries `key\nvalue\0` (a key alone is
/// true). The last of a key wins, as in git.
pub(super) fn parse(list: &str) -> Signing {
    let entries: Vec<(String, Option<&str>)> = list
        .split('\0')
        .filter(|e| !e.is_empty())
        .map(|e| match e.split_once('\n') {
            Some((key, value)) => (key.to_lowercase(), Some(value)),
            None => (e.to_lowercase(), None),
        })
        .collect();
    let get_any = |keys: &[&str]| {
        entries
            .iter()
            .rev()
            .find(|(k, _)| keys.contains(&k.as_str()))
            .map(|(_, v)| *v)
    };
    let get = |key: &str| get_any(&[key]);
    let text = |key: &str| get(key).flatten().unwrap_or_default().to_owned();
    let flag = |key: &str| {
        get(key).is_some_and(|v| {
            v.is_none_or(|v| ["true", "yes", "on", "1"].contains(&v.trim().to_lowercase().as_str()))
        })
    };
    let format = text("gpg.format");
    let program = get_any(program_keys(&format))
        .flatten()
        .unwrap_or_default()
        .to_owned();
    Signing {
        key: text("user.signingkey"),
        program,
        sign_commits: flag("commit.gpgsign"),
        sign_tags: flag("tag.gpgsign"),
        format,
    }
}

/// Reads the signing settings of the config `scope` names (`--global`, or `--file <path>`).
pub(super) fn read(scope: &[&str]) -> Signing {
    let mut args = vec!["config"];
    args.extend(scope);
    args.extend(["--list", "-z"]);
    // No config file at all is nothing set.
    crate::git::run(None, &args)
        .map(|out| parse(&out))
        .unwrap_or_default()
}

/// Sets `key` in the config `scope` names, or takes it out when `value` is empty. A key set
/// more than once becomes set once: git refuses to overwrite several values with one.
fn set(scope: &[&str], key: &str, value: &str) -> Result<(), String> {
    let mut args = vec!["config"];
    args.extend(scope);
    if value.is_empty() {
        args.extend(["--unset-all", key]);
        // 5: it wasn't set.
        crate::git::run_ok_codes(None, &args, &[0, 5]).map(drop)
    } else {
        args.extend(["--replace-all", key, value]);
        crate::git::run(None, &args).map(drop)
    }
}

/// Writes what differs between `before` and `after` to the config `scope` names. The program
/// is that of `after`'s format; a new format with the same program keeps the one set for it.
pub(super) fn write(scope: &[&str], before: &Signing, after: &Signing) -> Result<(), String> {
    let flag = |on: bool| if on { "true" } else { "false" };
    if after.format() != before.format() {
        set(scope, "gpg.format", &after.format)?;
    }
    if after.program != before.program
        && let Some((key, others)) = program_keys(after.format()).split_first()
    {
        // Written first: should that fail, the program set under another key is still there.
        set(scope, key, after.program.trim())?;
        for other in others {
            // It would win over the one written.
            set(scope, other, "")?;
        }
    }
    if after.key != before.key {
        set(scope, "user.signingkey", after.key.trim())?;
    }
    if after.sign_commits != before.sign_commits {
        set(scope, "commit.gpgSign", flag(after.sign_commits))?;
    }
    if after.sign_tags != before.sign_tags {
        set(scope, "tag.gpgSign", flag(after.sign_tags))?;
    }
    Ok(())
}

/// What the settings page holds between frames: the values as read, as being edited, and the
/// last error writing them.
#[derive(Clone)]
struct Form {
    saved: Signing,
    edit: Signing,
    read_at: Instant,
    error: Option<String>,
}

/// The signing fields as rows of `card`, editing `edit`; `salt` keeps the ids of two forms
/// apart. Returns true when a choice changed or a text field was left: when to write what was
/// typed.
pub(super) fn fields_rows(card: &mut Card, edit: &mut Signing, salt: &str) -> bool {
    let mut commit = false;
    card.row("Format", "gpg openpgp ssh x509 s/mime", |ui| {
        let shown = FORMATS
            .iter()
            .find(|(value, _)| *value == edit.format())
            .map_or(edit.format(), |(_, label)| label);
        egui::ComboBox::from_id_salt((salt, "format"))
            .width(layout::fit(ui, super::COMBO_WIDTH))
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (value, label) in FORMATS {
                    if ui.selectable_label(edit.format() == value, label).clicked()
                        && edit.format() != value
                    {
                        // Read back with the program set for it.
                        edit.format = value.to_owned();
                        commit = true;
                    }
                }
            })
            .response
            .on_hover_text("gpg.format");
    });

    let (key_hint, program_hint) = match edit.format() {
        "ssh" => (crate::i18n::text("interaction-tools-ssh-key"), "ssh-keygen"),
        "x509" => (crate::i18n::text("interaction-tools-certificate"), "gpgsm"),
        _ => (crate::i18n::text("interaction-tools-key-id"), "gpg"),
    };
    card.row_hint(
        "Signing key",
        crate::i18n::text("interaction-tools-identity"),
        "signingkey gpg ssh",
        |ui| {
            let key = ui
                .add(
                    layout::field(&mut edit.key)
                        .id_salt((salt, "key"))
                        .hint_text(key_hint)
                        .desired_width(layout::fit(ui, super::FIELD_WIDTH)),
                )
                .on_hover_text(crate::i18n::text("interaction-tools-key-help"));
            commit |= key.lost_focus();
        },
    );

    card.row_hint(
        "Program",
        crate::i18n::text("interaction-tools-path"),
        "gpg ssh-keygen gpgsm",
        |ui| {
            let program = ui
                .add(
                    layout::field(&mut edit.program)
                        .id_salt((salt, "program"))
                        .hint_text(program_hint)
                        .desired_width(layout::fit(ui, super::FIELD_WIDTH)),
                )
                .on_hover_text(crate::i18n::message(
                    "interaction-tools-program",
                    &[
                        ("key", program_key(edit.format()).into()),
                        ("program", program_hint.into()),
                    ],
                ));
            commit |= program.lost_focus();
        },
    );

    card.row("Sign by default", "commits tags gpgsign", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            commit |= ui
                .checkbox(
                    &mut edit.sign_commits,
                    crate::i18n::text("interaction-tools-commits"),
                )
                .on_hover_text("commit.gpgSign")
                .changed();
            commit |= ui
                .checkbox(
                    &mut edit.sign_tags,
                    crate::i18n::text("interaction-tools-tags"),
                )
                .on_hover_text(crate::i18n::text("interaction-tools-tag-help"))
                .changed();
        });
    });
    commit
}

const GLOBAL: &[&str] = &["--global"];

/// The commit signing form. Choices are written at once, text fields when they lose focus.
/// `overridden_by`: the profile in use, when it signs its own way, which wins over this form.
pub(super) fn signing_ui(ui: &mut Ui, filter: &Filter, overridden_by: Option<&str>) {
    let t = Tokens::get(ui.ctx());
    let id = egui::Id::new("settings-signing");
    let mut form = ui
        .data(|d| d.get_temp::<Form>(id))
        .filter(|f| f.edit != f.saved || f.read_at.elapsed() < RECHECK)
        .unwrap_or_else(|| {
            let saved = read(GLOBAL);
            Form {
                edit: saved.clone(),
                saved,
                read_at: Instant::now(),
                error: None,
            }
        });
    let mut commit = false;
    section(
        ui,
        filter,
        "Commit signing",
        Some(crate::i18n::text("interaction-tools-global")),
        |card| {
            if let Some(profile) = overridden_by {
                card.full("profile own override", |ui| {
                    ui.label(
                        RichText::new(crate::i18n::message(
                            "interaction-tools-profile-override",
                            &[("profile", profile.into())],
                        ))
                        .color(t.warning)
                        .small(),
                    );
                });
            }
            commit |= fields_rows(card, &mut form.edit, "settings-signing");
            if let Some(e) = &form.error {
                card.full("", |ui| ui.label(RichText::new(e).color(t.danger).small()));
            }
        },
    );
    if commit && form.edit != form.saved {
        form.error = write(GLOBAL, &form.saved, &form.edit).err();
        form.saved = read(GLOBAL);
        form.read_at = Instant::now();
        if form.error.is_none() {
            form.edit = form.saved.clone();
        }
    }
    ui.data_mut(|d| d.insert_temp(id, form));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_signing_fields_translate_without_reading_or_writing_config() {
        use egui_kittest::{Harness, kittest::Queryable};
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, edit: &mut Signing| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let filter = Filter::new("", "Commit signing", false);
                section(ui, &filter, "Commit signing", None, |card| {
                    fields_rows(card, edit, "inert-signing-form");
                });
            },
            Signing {
                format: "ssh".into(),
                key: "key::ssh-ed25519 日本語".into(),
                ..Signing::default()
            },
        );
        h.run();
        h.get_by_label("Signaturschlüssel");
        h.get_by_label("Standardmäßig signieren");
        h.get_by_label("Commits").click();
        h.run();
        assert!(h.state().sign_commits);
        assert_eq!(h.state().format, "ssh");
        assert_eq!(h.state().key, "key::ssh-ed25519 日本語");
        assert_eq!(program_key(h.state().format()), "gpg.ssh.program");
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Signing key");
        assert!(h.state().sign_commits);
        assert_eq!(h.state().key, "key::ssh-ed25519 日本語");
    }

    #[test]
    fn signing_settings_in_a_config_list() {
        let list = "user.name\nMe\0gpg.format\nssh\0user.signingkey\n~/.ssh/id.pub\0\
                    gpg.program\ngpg2\0gpg.ssh.program\nC:/ssh/ssh-keygen.exe\0\
                    commit.gpgsign\0tag.gpgsign\nfalse\0tag.gpgsign\nYes\0";
        assert_eq!(
            parse(list),
            Signing {
                format: "ssh".into(),
                key: "~/.ssh/id.pub".into(),
                program: "C:/ssh/ssh-keygen.exe".into(),
                sign_commits: true,
                sign_tags: true,
            }
        );
        let plain = parse("gpg.program\ngpg2\0commit.gpgsign\nfalse\0");
        assert_eq!(
            (plain.format(), plain.program.as_str()),
            ("openpgp", "gpg2")
        );
        assert!(!plain.sign_commits && !plain.sign_tags);
        assert_eq!(parse(""), Signing::default());
    }

    /// Against a scratch config file, never the user's own.
    #[test]
    fn signing_settings_are_written_and_read_back() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-signing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join("config");
        let file = file.to_string_lossy();
        let scope = ["--file", &*file];
        assert_eq!(read(&scope), Signing::default());

        let ssh = Signing {
            format: "ssh".into(),
            key: "key::ssh-ed25519 AAAA me".into(),
            program: "ssh-keygen".into(),
            sign_commits: true,
            sign_tags: false,
        };
        write(&scope, &Signing::default(), &ssh)?;
        assert_eq!(read(&scope), ssh);

        // Back to OpenPGP without a key: the key goes, SSH's program stays SSH's.
        let openpgp = Signing {
            format: "openpgp".into(),
            sign_tags: true,
            ..Signing::default()
        };
        write(
            &scope,
            &ssh,
            &Signing {
                program: ssh.program.clone(),
                ..openpgp.clone()
            },
        )?;
        assert_eq!(read(&scope), openpgp);
        let gpg = Signing {
            program: "gpg2".into(),
            ..openpgp.clone()
        };
        write(&scope, &openpgp, &gpg)?;
        assert_eq!(read(&scope), gpg);
        let text = std::fs::read_to_string(dir.join("config")).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!text.contains("signingkey"), "{text}");
        assert!(text.contains("program = ssh-keygen"), "{text}");
        assert!(text.contains("program = gpg2"), "{text}");
        Ok(())
    }

    /// OpenPGP's program under its newer name, and keys set twice in a hand-edited config.
    #[test]
    fn signing_settings_set_twice_or_under_the_newer_name() -> Result<(), String> {
        let newer = parse("gpg.program\ngpg2\0gpg.openpgp.program\n/opt/gpg\0");
        assert_eq!(newer.program, "/opt/gpg");

        let dir = std::env::temp_dir().join(format!("gitcraft-signing2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("config");
        std::fs::write(
            &path,
            "[commit]\n\tgpgsign = true\n[commit]\n\tgpgsign = false\n\
             [gpg \"openpgp\"]\n\tprogram = /opt/gpg\n",
        )
        .map_err(|e| e.to_string())?;
        let file = path.to_string_lossy();
        let scope = ["--file", &*file];
        let before = read(&scope);
        assert_eq!(before.program, "/opt/gpg");
        let after = Signing {
            program: "gpg2".into(),
            sign_commits: true,
            ..before.clone()
        };
        let written = write(&scope, &before, &after);
        let back = read(&scope);
        let _ = std::fs::remove_dir_all(&dir);
        written?;
        assert_eq!(back, after);
        Ok(())
    }
}
