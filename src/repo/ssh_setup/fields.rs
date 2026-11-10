//! SSH setup controls for key generation, adoption, registration, and testing.

use super::*;

impl State {
    pub(super) fn fields(&mut self, ui: &mut Ui) {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-ssh_setup-fields-4405a792",
                &[(
                    "self-machine-label-1",
                    (self.machine.label()).clone().into(),
                )],
            ))
            .strong(),
        );
        if !self.machine.is_local() {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-69608cb8",
            ));
        }
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-a7b7ae57",
        ));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-08b5218c",
            ));
            ui.text_edit_singleline(&mut self.folder);
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-25f08261",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-e67b66d2",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Generate);
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-a8ff2446",
            ));
            ui.text_edit_singleline(&mut self.key);
            if self.machine.is_local()
                && ui
                    .button(crate::i18n::text(
                        "repo-workflows-ssh_setup-fields-42ad2d5a",
                    ))
                    .clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.key = path.to_string_lossy().into_owned();
            }
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-a6faa7c0",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-1cb4035b",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Agent);
        }
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-75c9835b",
        ));
        ui.separator();
        let old = (self.forge, self.host.clone(), self.account_name.clone());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("ssh_setup_forge")
                .selected_text(self.forge.name())
                .show_ui(ui, |ui| {
                    for forge in [Forge::GitHub, Forge::Forgejo, Forge::Gitea] {
                        ui.selectable_value(&mut self.forge, forge, forge.name());
                    }
                });
            if self.machine.is_local() || self.forge == Forge::GitHub {
                ui.label(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-467c73ec",
                ));
                ui.text_edit_singleline(&mut self.host);
            } else {
                ui.label(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-2fc162ec",
                ));
            }
        });
        ui.horizontal(|ui| {
            ui.label(if self.forge == Forge::GitHub {
                crate::i18n::text("repo-workflows-ssh_setup-fields-fa174850")
            } else {
                crate::i18n::text("repo-workflows-ssh_setup-fields-9c3b602e")
            });
            ui.text_edit_singleline(&mut self.account_name);
        });
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-2b81b1a8",
            ));
            ui.text_edit_singleline(&mut self.title);
        });
        if old != (self.forge, self.host.clone(), self.account_name.clone()) {
            self.account = None;
        }
        if self.machine.is_local() {
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-ssh_setup-fields-9487c9a7",
                    ))
                    .clicked()
                {
                    match self.hosting() {
                        Ok(hosting) => {
                            let name = self.account_name.clone();
                            self.background(ui.ctx(), move |live| {
                                ssh_keys::prepare(&hosting, &name, live).map(Ready::Account)
                            });
                        }
                        Err(e) => self.status = e,
                    }
                }
                if ui
                    .add_enabled(
                        self.account.is_some(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-ssh_setup-fields-71b7ffa7",
                        )),
                    )
                    .clicked()
                    && let Some(account) = self.account.take()
                {
                    let machine = self.machine.clone();
                    let key = self.key.clone();
                    let title = self.title.clone();
                    self.background(ui.ctx(), move |live| {
                        let path = public_companion(&machine, &key)?;
                        let public = ssh_keys::public_key::read(&path)?;
                        account.upload(&title, &public, live)?;
                        Ok(Ready::Uploaded)
                    });
                }
            });
            if let Some(account) = &self.account {
                ui.label(crate::i18n::message(
                    "repo-workflows-ssh_setup-fields-0aaeaa7d",
                    &[
                        ("account-owner-1", (account.owner()).into()),
                        ("self-host-2", (self.host).clone().into()),
                    ],
                ));
            }
        } else {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-c362f511",
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-005427d8",
                ))
                .clicked()
            {
                self.prepare_command(ui.ctx(), Step::Register);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-76f62ede",
            ));
            ui.text_edit_singleline(&mut self.destination);
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-391d0d15",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-4a4bcca6",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Test);
        }
        if self.machine.is_local()
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-c62cdf9c",
                ))
                .clicked()
        {
            let path = PathBuf::from(self.key.clone());
            let machine = self.machine.clone();
            let key = self.key.clone();
            self.background(ui.ctx(), move |_| {
                machine.validate_path(&key)?;
                let meta = std::fs::metadata(&path).map_err(|_| {
                    crate::i18n::text("repo-workflows-ssh_setup-fields-fdfcad0a").to_owned()
                })?;
                if !meta.is_file() {
                    return Err(
                        crate::i18n::text("repo-workflows-ssh_setup-fields-c37c192f").into(),
                    );
                }
                Ok(Ready::Adopt(path))
            });
        }
    }
}
