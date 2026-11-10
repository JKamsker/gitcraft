//! Unity setup mutations, ignore rules, attributes, and merge-driver registration.

use super::*;

impl RepoTab {
    /// Runs `work` in the background (`doing` says what, while it runs), says what it did
    /// (`done`) or why it failed, then looks at the project again.
    pub(super) fn unity_job(
        &mut self,
        (doing, done): (&str, String),
        ctx: &egui::Context,
        work: impl FnOnce(&std::path::Path) -> Result<(), String> + Send + 'static,
    ) {
        if self.unity.busy.is_some() {
            return;
        }
        self.unity.busy = Some(doing.to_owned());
        let failed = crate::i18n::message(
            "repo-workflows-unity-unity-job-bdc09466",
            &[("doing", (doing).into())],
        );
        let path = self.path.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || caught(|| work(&path)),
            move |tab, result, ctx| {
                tab.unity.busy = None;
                tab.status = Some(crate::repo::Notice::plain(match result {
                    Ok(()) => (done, false),
                    Err(e) => (format!("{failed}: {e}"), true),
                }));
                tab.unity.detected_for = None;
                tab.reload_status(ctx);
            },
        );
    }

    pub(super) fn add_attributes(&mut self, lfs: bool, ctx: &egui::Context) {
        let Some(Ok(setup)) = &self.unity.setup else {
            return;
        };
        let missing = if lfs {
            setup.missing_lfs.clone()
        } else {
            setup.missing_yaml.clone()
        };
        if missing.is_empty() {
            return;
        }
        let block = git::attributes_block(&missing);
        let done = crate::i18n::message(
            "repo-workflows-unity-add-attributes-ec651afe",
            &[("missing-len-1", (missing.len()).into())],
        );
        self.unity_job(("Adding lines to .gitattributes", done), ctx, move |repo| {
            git::append_block(repo, ".gitattributes", &block)
        });
    }

    pub(super) fn add_ignores(&mut self, ctx: &egui::Context) {
        let (Some(Ok(setup)), Some(project)) = (&self.unity.setup, &self.unity.project) else {
            return;
        };
        let block = git::ignore_block(project, &setup.unignored);
        let done = crate::i18n::message(
            "repo-workflows-unity-add-ignores-e8e18aea",
            &[(
                "setup-unignored-join-1",
                (setup.unignored.join(", ")).clone().into(),
            )],
        );
        self.unity_job(("Adding lines to .gitignore", done), ctx, move |repo| {
            git::append_block(repo, ".gitignore", &block)
        });
    }

    pub(super) fn untrack_generated(&mut self, ctx: &egui::Context) {
        let (Some(Ok(setup)), Some(project)) = (&self.unity.setup, self.unity.project.clone())
        else {
            return;
        };
        let folders = setup.tracked_generated.clone();
        let done = crate::i18n::message(
            "repo-workflows-unity-untrack-generated-fc9783a8",
            &[("folders-join-1", (folders.join(", ")).clone().into())],
        );
        self.unity_job(
            ("Taking generated folders out of git", done),
            ctx,
            move |repo| git::untrack_generated(repo, &project, &folders),
        );
    }

    /// Asks for UnityYAMLMerge with a file dialog (as the app's other dialogs are asked), and
    /// keeps the choice in the repository's config.
    pub(super) fn choose_tool(&mut self, ctx: &egui::Context) {
        let title = if cfg!(target_os = "macos") {
            crate::i18n::text("repo-workflows-unity-choose-tool-61dcc5f4")
        } else {
            crate::i18n::text("repo-workflows-unity-choose-tool-05ee7795")
        };
        let Some(file) = rfd::FileDialog::new().set_title(title).pick_file() else {
            return;
        };
        let name = file.file_name().map(|n| n.to_string_lossy().to_lowercase());
        if !name.is_some_and(|n| n.starts_with("unityyamlmerge")) {
            self.status = Some(crate::repo::Notice::plain((
                format!("{} isn't UnityYAMLMerge", file.display()),
                true,
            )));
            return;
        }
        let value = file.to_string_lossy().into_owned();
        let done = crate::i18n::message(
            "repo-workflows-unity-choose-tool-1861f75d",
            &[("value", (value).clone().into())],
        );
        self.unity_job(("Saving the choice", done), ctx, move |repo| {
            git::run(
                Some(repo),
                &["config", "--local", smart_merge::TOOL_KEY, &value],
            )
            .map(|_| ())
        });
    }

    pub(super) fn register_driver(&mut self, ctx: &egui::Context) {
        let Some(tool) = self.unity.tool.clone() else {
            self.status = Some(crate::repo::Notice::plain((
                "UnityYAMLMerge wasn't found: choose it first".to_owned(),
                true,
            )));
            return;
        };
        let done = crate::i18n::text("repo-workflows-unity-register-driver-57fdd53f").to_owned();
        self.unity_job(("Registering UnityYAMLMerge", done), ctx, move |repo| {
            smart_merge::register(repo, &tool)
        });
    }
}
