//! Hotfix operation commands, undo, redo and review preview.

use super::*;

impl Finish {
    /// The branches it merges into.
    pub(super) fn targets(&self, c: &Choices) -> Vec<&At> {
        std::iter::once(&self.main)
            .chain(self.develop.as_ref().filter(|_| c.develop))
            .collect()
    }

    /// Why it can't be finished as chosen, if it can't.
    pub(crate) fn problem(&self, c: &Choices) -> Option<String> {
        let tag = c.tag.trim();
        if let Some(p) = crate::release::tag_problem(tag, &self.tags) {
            return Some(p);
        }
        let conflicts = self.conflicts.as_ref()?;
        let names: Vec<&str> = self.targets(c).iter().map(|t| t.name.as_str()).collect();
        conflicts
            .iter()
            .find(|(into, _)| names.contains(&into.as_str()))
            .map(|(into, files)| {
                let or = if *into == self.main.name {
                    ""
                } else {
                    crate::i18n::text("repo-workflows-hotfix-problem-a56c91b5")
                };
                crate::i18n::message(
                    "repo-workflows-hotfix-problem-0225cdbc",
                    &[
                        ("into", (into).clone().into()),
                        ("files-join-1", (files.join(", ")).clone().into()),
                        ("or", (or).into()),
                    ],
                )
            })
    }

    /// The local steps, as one operation: only while every branch is where the plan saw it.
    pub(crate) fn steps(&self, c: &Choices) -> Vec<Vec<String>> {
        let h = &self.hotfix;
        let hotfix_ref = format!("refs/heads/{}", h.name);
        let mut steps = vec![
            expect_symref("HEAD", &hotfix_ref),
            expect_ref_at(&hotfix_ref, &h.head),
        ];
        for t in self.targets(c) {
            steps.push(expect_ref_at(&format!("refs/heads/{}", t.name), &t.head));
        }
        // Into the development branch first, so that it ends on the main one.
        for t in self.targets(c).into_iter().rev() {
            steps.push(strings(&["checkout", &t.name, "--"]));
            steps.push(strings(&[
                "merge",
                "--no-ff",
                "-m",
                &merge_message(&h.name, &t.name, &self.main.name),
                &hotfix_ref,
            ]));
        }
        steps.push(self.tag_step(c));
        if c.delete {
            steps.push(strings(&["branch", "-D", "--", &h.name]));
        }
        steps
    }

    /// The tag, on the main branch's merge (as git-flow tags it): signed only when asked, even
    /// where `tag.gpgSign` is set.
    pub(super) fn tag_step(&self, c: &Choices) -> Vec<String> {
        let message = if c.notes.trim().is_empty() {
            c.tag.trim().to_owned()
        } else {
            format!("{}\n\n{}", c.tag.trim(), c.notes.trim())
        };
        let mut step = Vec::new();
        if !c.sign {
            step.extend(strings(&["-c", "tag.gpgSign=false"]));
        }
        step.extend(strings(&[
            "tag",
            if c.sign { "-s" } else { "-a" },
            "--cleanup=whitespace",
            "-m",
            &message,
            "--",
            c.tag.trim(),
            &format!("refs/heads/{}", self.main.name),
        ]));
        step
    }

    /// How the local steps are undone: the hotfix branch back (checked out), the branches
    /// merged into back where they were, the tag gone; only while they are as it left them.
    pub(crate) fn undo(&self, c: &Choices) -> Vec<Vec<String>> {
        let h = &self.hotfix;
        let mut undo = vec![expect_symref(
            "HEAD",
            &format!("refs/heads/{}", self.main.name),
        )];
        for t in self.targets(c) {
            undo.push(expect_ref(&format!("refs/heads/{}", t.name)));
        }
        undo.push(expect_ref(&format!("refs/tags/{}", c.tag.trim())));
        if c.delete {
            // Not made again since.
            undo.push(expect_ref_at(&format!("refs/heads/{}", h.name), ""));
            undo.push(strings(&["branch", &h.name, &h.head]));
            undo.extend(upstream_back(h));
        }
        undo.push(strings(&["checkout", &h.name, "--"]));
        for t in self.targets(c) {
            undo.push(strings(&[
                "update-ref",
                &format!("refs/heads/{}", t.name),
                &t.head,
            ]));
        }
        undo.push(strings(&["tag", "-d", c.tag.trim()]));
        undo
    }

    /// How redoing puts back what finishing made (the same merges, the same tag) rather than
    /// make new ones, which would no longer be what was pushed: `merged` are where the branches
    /// merged into ended, `tag` the tag's object. Only from where undoing left things.
    pub(crate) fn redo(
        &self,
        c: &Choices,
        merged: &[(String, String)],
        tag: &str,
    ) -> Vec<Vec<String>> {
        let h = &self.hotfix;
        let hotfix_ref = format!("refs/heads/{}", h.name);
        let mut redo = vec![
            expect_symref("HEAD", &hotfix_ref),
            expect_ref_at(&hotfix_ref, &h.head),
        ];
        for t in self.targets(c) {
            redo.push(expect_ref_at(&format!("refs/heads/{}", t.name), &t.head));
        }
        let tag_ref = format!("refs/tags/{}", c.tag.trim());
        redo.push(expect_ref_at(&tag_ref, ""));
        for (name, at) in merged {
            let old = self
                .targets(c)
                .iter()
                .find(|t| &t.name == name)
                .map(|t| t.head.clone())
                .unwrap_or_default();
            redo.push(strings(&[
                "update-ref",
                &format!("refs/heads/{name}"),
                at,
                &old,
            ]));
        }
        redo.push(strings(&["update-ref", &tag_ref, tag, ""]));
        redo.push(strings(&["checkout", &self.main.name, "--"]));
        if c.delete {
            redo.push(strings(&["branch", "-D", "--", &h.name]));
        }
        redo
    }

    /// The push of the branches merged into and the tag, all or nothing.
    pub(crate) fn push(&self, c: &Choices) -> Option<Vec<String>> {
        let remote = self.remote.as_deref().filter(|_| c.push)?;
        let mut args = strings(&["push", "--atomic", "--", remote]);
        for t in self.targets(c) {
            args.push(format!("refs/heads/{}:{}", t.name, t.remote_ref));
        }
        args.push(format!("refs/tags/{0}:refs/tags/{0}", c.tag.trim()));
        Some(args)
    }

    /// What each step will do, in words, for the dialog.
    pub(crate) fn preview(&self, c: &Choices) -> Vec<String> {
        let h = &self.hotfix;
        let mut out = Vec::new();
        for t in self.targets(c).into_iter().rev() {
            out.push(crate::i18n::message(
                "repo-workflows-hotfix-preview-ab279563",
                &[
                    ("h-name-1", (h.name).clone().into()),
                    (
                        "crate-repo-short-h-head-2",
                        (crate::repo::short(&h.head)).into(),
                    ),
                    ("t-name-3", (t.name).clone().into()),
                ],
            ));
        }
        out.push(crate::i18n::message(
            "repo-workflows-hotfix-preview-523cd105",
            &[
                (
                    "if-c-sign-signed-else-annotated-1",
                    (if c.sign { "signed" } else { "annotated" }).into(),
                ),
                ("c-tag-trim-2", (c.tag.trim()).into()),
                ("self-main-name-3", (self.main.name).clone().into()),
            ],
        ));
        if c.delete {
            out.push(crate::i18n::message(
                "repo-workflows-hotfix-preview-4aeab9fa",
                &[("h-name-1", (h.name).clone().into())],
            ));
        }
        if let Some(remote) = self.remote.as_deref().filter(|_| c.push) {
            let names: Vec<String> = self
                .targets(c)
                .iter()
                .map(|t| {
                    t.remote_ref
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&t.remote_ref)
                        .to_owned()
                })
                .collect();
            out.push(crate::i18n::message(
                "repo-workflows-hotfix-preview-7df6bedd",
                &[
                    (
                        "names-join-and-1",
                        (names.join(crate::i18n::text("repo-workflows-hotfix-preview-e3ee915a")))
                            .clone()
                            .into(),
                    ),
                    ("remote", (remote).into()),
                ],
            ));
        }
        out
    }
}
