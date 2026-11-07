use crate::{
    connections::{self, Account, Choices},
    portability::Provider,
    settings::UserProfile,
};
#[derive(Clone, Default)]
pub(super) struct Fresh {
    pub(super) ssh: String,
    pub(super) signing: String,
    folders: Vec<String>,
}
pub(super) fn fields(ui: &mut egui::Ui, profile: &UserProfile, fresh: &mut Fresh) -> bool {
    let Some(requirements) = &profile.pending_setup else {
        return false;
    };
    if requirements.ssh_key_reselection_required {
        ui.label("Select this machine's SSH private key (no key is read into the setup file)");
        if ui.button("Choose local SSH key…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                fresh.ssh = path.to_string_lossy().into_owned();
            }
        }
        ui.label(&fresh.ssh);
    }
    if requirements.signing_key_reselection_required {
        ui.label("Enter this machine's signing key identifier or local public-key path");
        ui.text_edit_singleline(&mut fresh.signing);
    }
    fresh
        .folders
        .resize(requirements.accounts.len(), String::new());
    for (a, folder) in requirements.accounts.iter().zip(&mut fresh.folders) {
        ui.label(format!(
            "Required {:?}: {} on {}",
            a.provider, a.user_or_alias, a.host
        ));
        if a.provider != Provider::Tea {
            ui.label(
                "Optional local CLI authentication folder (empty uses this machine's CLI store)",
            );
            ui.text_edit_singleline(folder);
        }
    }
    ui.button("Verify required setup and enable profile")
        .clicked()
}
fn local_key(path: &str) -> Result<(), String> {
    let path = std::path::Path::new(path);
    if !path.is_absolute() {
        return Err("Select a local absolute SSH key path".into());
    }
    let m = std::fs::metadata(path).map_err(|_| "Selected local SSH key is unavailable")?;
    if !m.is_file() {
        return Err("Selected SSH key must be a local file".into());
    }
    Ok(())
}
pub(super) fn verify(
    profile: &UserProfile,
    fresh: &Fresh,
    active: &dyn Fn() -> bool,
) -> Result<UserProfile, String> {
    let requirements = profile
        .pending_setup
        .as_ref()
        .ok_or("Profile is no longer pending")?;
    let mut choices = Choices::default();
    if requirements.ssh_key_reselection_required {
        local_key(&fresh.ssh)?;
        choices.ssh_key = fresh.ssh.clone();
    }
    if requirements.signing_key_reselection_required
        && (fresh.signing.trim().is_empty() || fresh.signing.chars().any(char::is_control))
    {
        return Err("Choose this machine's signing key".into());
    }
    for (index, a) in requirements.accounts.iter().enumerate() {
        if !active() {
            return Err("Profile setup cancelled".into());
        }
        let account = Account {
            host: a.host.clone(),
            user: a.user_or_alias.clone(),
            config_dir: fresh.folders.get(index).cloned().unwrap_or_default(),
        };
        match a.provider {
            Provider::Github => {
                connections::check_account("gh", &account)?;
                choices.github = account;
            }
            Provider::Gitlab => {
                connections::check_account("glab", &account)?;
                choices.gitlab = account;
            }
            Provider::Tea => {
                let (host, user) = crate::forge::selected_tea_metadata(&account.user)
                    .ok_or("The expected local tea login is unavailable")?;
                if !host.eq_ignore_ascii_case(&a.host) {
                    return Err("The tea login belongs to a different HTTPS server".into());
                }
                let expected=a.account_user.as_ref().ok_or("The imported tea expectation has no public owner; export it again from the original machine")?;
                if !user.eq_ignore_ascii_case(expected) {
                    return Err("The selected tea alias belongs to a different account user".into());
                }
                connections::check_account("tea", &account)?;
                choices.tea_login = account.user;
                choices.tea_host = host;
            }
        }
    }
    choices.validate()?;
    if !active() {
        return Err("Profile setup cancelled".into());
    }
    let mut ready = profile.clone();
    ready.connections = choices;
    if requirements.signing_key_reselection_required {
        ready.signing.key = fresh.signing.clone();
        ready.signing.program.clear();
    }
    ready.pending_setup = None;
    Ok(ready)
}
