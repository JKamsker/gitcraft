//! Discovery supplies clone addresses, never API credentials.
use super::*;
pub(super) struct CloneSource {
    https: String,
    ssh: String,
}
impl App {
    pub(in crate::app) fn start_discovered_clone(&mut self, https: String, ssh: String) {
        self.start_clone();
        self.set_clone_url(&https);
        if let Some(form) = &mut self.clone_form {
            form.source = Some(CloneSource { https, ssh });
        }
    }
}
pub(super) fn controls(ui: &mut Ui, form: &mut CloneForm) {
    let Some(source) = &form.source else {
        return;
    };
    let mut ssh = form.url == source.ssh;
    let before = ssh;
    ui.horizontal(|ui| {
        ui.radio_value(&mut ssh, false, "HTTPS");
        ui.radio_value(&mut ssh, true, "SSH");
    });
    if before != ssh {
        form.url = if ssh {
            source.ssh.clone()
        } else {
            source.https.clone()
        };
        if !form.name_edited {
            form.name = git::clone_dir_name(&form.url);
        }
        form.auth = false;
        form.username.clear();
        form.password.clear();
        form.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    fn form() -> CloneForm {
        CloneForm {
            source: Some(CloneSource {
                https: "https://github.com/owner/repo.git".into(),
                ssh: "git@github.com:owner/repo.git".into(),
            }),
            open_after: true,
            options: Default::default(),
            depth: String::new(),
            sparse_folders: String::new(),
            url: "https://github.com/owner/repo.git".into(),
            parent: "/chosen/destination".into(),
            name: "custom-name".into(),
            name_edited: true,
            error: None,
            focus: false,
            auth: true,
            username: "old-user".into(),
            password: "old-secret".into(),
        }
    }
    #[test]
    fn discovered_clone_transport_preserves_destination_and_clears_retry_credentials() {
        let mut harness = Harness::new_ui_state(
            |ui, form: &mut CloneForm| {
                controls(ui, form);
                ui.checkbox(&mut form.open_after, "Open after clone");
            },
            form(),
        );
        harness.run();
        harness.get_by_label("SSH").click();
        harness.run();
        assert_eq!(harness.state().url, "git@github.com:owner/repo.git");
        assert_eq!(harness.state().name, "custom-name");
        assert_eq!(harness.state().parent, "/chosen/destination");
        assert!(harness.state().password.is_empty());
        assert!(!harness.state().auth);
        harness.get_by_label("Open after clone").click();
        harness.run();
        assert!(!harness.state().open_after);
        harness.get_by_label("HTTPS").click();
        harness.run();
        assert_eq!(harness.state().url, "https://github.com/owner/repo.git");
        assert!(!harness.state().open_after);
    }
}
