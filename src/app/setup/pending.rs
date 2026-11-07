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
        ui.label("Select this machine's explicit SSH key file (its matching private key may be in your local agent). Agent-only defaults and SSH-config aliases are not restored; no key is read into the setup file.");
        if ui.button("Choose local SSH key…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_file()
        {
            fresh.ssh = path.to_string_lossy().into_owned();
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
        if matches!(a.provider, Provider::Github | Provider::Gitlab | Provider::Azure) {
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
            Provider::Azure | Provider::Bitbucket => {
                let service=a.provider.cloud_service().ok_or("Unknown cloud provider")?;
                let account=connections::CloudAccount {
                    server:a.host.clone(),user:a.user_or_alias.clone(),
                    subject:a.account_user.clone().ok_or("The imported provider expectation has no verified public owner")?,
                    credential_user:a.credential_user.clone().unwrap_or_default(),
                    config_dir:if a.provider==Provider::Azure {account.config_dir}else{String::new()},
                };
                let verified=crate::forge::verify_cloud_account(service,&account)?;
                match a.provider {Provider::Azure=>choices.azure=verified,Provider::Bitbucket=>choices.bitbucket=verified,_=>return Err("Unknown cloud provider".into())}
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tea_round_trip_pins_public_owner_and_host_without_exporting_token_or_auth_folder()
    -> Result<(), String> {
        const CHILD: &str = "GITCRAFT_PORTABLE_TEA_FIXTURE";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let config = root.join("tea/config.yml");
            let write = |host: &str, user: &str| {
                std::fs::write(&config,format!("logins:\n- name: work\n  url: https://{host}\n  user: {user}\n  token: FIXTURE_TOKEN_MUST_NOT_EXPORT\n")).map_err(|e|e.to_string())
            };
            write("forge.fixture.invalid", "expected-work-user")?;
            let mut settings = crate::settings::Settings::default();
            settings.user_profiles[0].connections.tea_login = "work".into();
            settings.user_profiles[0].connections.tea_host = "forge.fixture.invalid".into();
            let mut file = crate::portability::Setup::new(&settings, &Default::default());
            file.resolve_public_accounts()?;
            let bytes = file.encode()?;
            assert!(!String::from_utf8_lossy(&bytes).contains("FIXTURE_TOKEN"));
            assert!(!String::from_utf8_lossy(&bytes).contains(root.to_str().ok_or("UTF-8 root")?));
            let profile = crate::portability::Setup::decode(&bytes)?.profiles[0].materialize(99);
            assert!(profile.pending_setup());
            let ready = verify(&profile, &Fresh::default(), &|| true)?;
            assert_eq!(ready.connections.tea_login, "work");
            assert!(!ready.pending_setup());
            write("forge.fixture.invalid", "different-personal-user")?;
            assert!(
                verify(&profile, &Fresh::default(), &|| true)
                    .is_err_and(|e| e.contains("different account"))
            );
            write("different.fixture.invalid", "expected-work-user")?;
            assert!(
                verify(&profile, &Fresh::default(), &|| true)
                    .is_err_and(|e| e.contains("different HTTPS"))
            );
            return Ok(());
        }
        let repo = crate::practice::create(crate::practice::Scenario::Published)?;
        let root = repo.parent().ok_or("root")?.join("portable-tea-config");
        std::fs::create_dir_all(root.join("tea")).map_err(|e| e.to_string())?;
        let output=std::process::Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","app::setup::pending::tests::tea_round_trip_pins_public_owner_and_host_without_exporting_token_or_auth_folder","--nocapture"]).env(CHILD,&root).env("XDG_CONFIG_HOME",&root).output().map_err(|e|e.to_string())?;
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(repo.parent().ok_or("root")?).map_err(|e| e.to_string())?;
        Ok(())
    }
}
