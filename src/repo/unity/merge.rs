//! UnityYAMLMerge conflict execution and editor completion handling.

use super::*;

impl RepoTab {
    /// Lets UnityYAMLMerge solve the open conflict; its result goes into the conflict editor.
    pub(super) fn smart_merge_conflict(&mut self, ctx: &egui::Context) {
        let Some(tool) = self.unity.tool.clone() else {
            self.status = Some(crate::repo::Notice::plain((
                "UnityYAMLMerge wasn't found: choose it in Unity set-up…".to_owned(),
                true,
            )));
            return;
        };
        let Some(ed) = self.conflict.as_mut().filter(|ed| !ed.solving) else {
            return;
        };
        ed.solving = true;
        let (repo, file) = (self.path.clone(), ed.path.clone());
        self.then(
            ctx,
            move || {
                let result = caught(|| smart_merge::solve(&repo, &file, &tool));
                (file, result)
            },
            |tab, (path, result), _| tab.smart_merge_done(&path, result),
        );
    }

    /// Takes UnityYAMLMerge's answer into the conflict editor, if it is still on that file.
    pub(super) fn smart_merge_done(&mut self, path: &str, result: Result<String, String>) {
        let Some(ed) = self.conflict.as_mut().filter(|ed| ed.path == path) else {
            return;
        };
        ed.solving = false;
        let text = match result {
            Ok(text) => text,
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((e, true)));
                return;
            }
        };
        let left = ed.take_solved(text);
        self.status = Some(crate::repo::Notice::plain(match left {
            0 => (
                format!("UnityYAMLMerge merged {path}: check the result, then save"),
                false,
            ),
            n => (
                format!("UnityYAMLMerge left {n} conflicts in {path} to choose"),
                false,
            ),
        }));
    }
}
