use super::*;

/// Ownership retained while a partial patch is being written, possibly over SSH/WSL.
/// A read epoch changes at reload start, before new data (or a read error) can arrive.
pub(in crate::repo) struct PreparedPick {
    path: PathBuf,
    read: u64,
    epoch: u64,
    key: DiffKey,
    request: u64,
    pick: diff::Pick,
    label: String,
    preparing: String,
    started: Instant,
}

impl RepoTab {
    /// Stages, unstages or discards the chosen lines of the open working-tree diff.
    pub(in crate::repo) fn pick_lines(
        &mut self,
        pick: diff::Pick,
        lines: &std::collections::BTreeSet<usize>,
        ctx: &egui::Context,
    ) {
        let Some((patch, prepared)) = self.prepare_picked(pick, lines) else {
            return;
        };
        let repo = prepared.path.clone();
        self.here_or_later(
            ctx,
            move || git::save_patch_for(&repo, &patch),
            move |tab, saved, ctx| tab.picked_prepared(prepared, saved, ctx),
        );
    }

    /// Capture the displayed patch and its ownership before starting any asynchronous write.
    pub(in crate::repo) fn prepare_picked(
        &mut self,
        pick: diff::Pick,
        lines: &std::collections::BTreeSet<usize>,
    ) -> Option<(String, PreparedPick)> {
        if self.busy_op.is_some()
            || self.loading
            || self.load_error.is_some()
            || self.work_pending != 0
        {
            return None;
        }
        let view = self.diff.as_ref()?;
        let (DiffKey::Work(file), Some(Ok(data))) = (&view.key, &view.data) else {
            return None;
        };
        if view.encrypted.enabled || view.request != view.data_request || !data.partial {
            return None;
        }
        let patch = diff::partial_patch(&data.header, &data.lines, lines, pick)?;
        let n = lines.len();
        let label = format!(
            "{} {n} line{} of {}",
            pick.verb(),
            if n == 1 { "" } else { "s" },
            file.path
        );
        let prepared = PreparedPick {
            path: self.path.clone(),
            read: self.work_read,
            epoch: self.work_epoch,
            key: view.key.clone(),
            request: view.request,
            pick,
            preparing: crate::i18n::message(
                "repo-core-0166",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&label).into(),
                )],
            ),
            label,
            started: Instant::now(),
        };
        self.busy_op = Some(prepared.preparing.clone());
        self.op_started = Some(prepared.started);
        Some((patch, prepared))
    }

    /// Complete the same preparation for mouse and keyboard actions. A pending or failed
    /// full read keeps retained diff/status identities, so those alone are not sufficient.
    pub(in crate::repo) fn picked_prepared(
        &mut self,
        prepared: PreparedPick,
        saved: Result<String, String>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.as_ref() != Some(&prepared.preparing)
            || self.op_started != Some(prepared.started)
        {
            return;
        }
        self.busy_op = None;
        let current = self.diff.as_ref().is_some_and(|view| {
            !view.encrypted.enabled
                && view.key == prepared.key
                && view.request == prepared.request
                && view.data_request == view.request
        });
        if self.path != prepared.path
            || self.work_read != prepared.read
            || self.work_epoch != prepared.epoch
            || self.work_pending != 0
            || self.loading
            || self.load_error.is_some()
            || !current
        {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-0167").to_owned(),
                true,
            )));
            return;
        }
        self.apply_picked(prepared.pick, &prepared.label, saved, ctx);
    }

    /// Applies the patch of lines picked, saved at `saved` (see [`Self::pick_lines`]).
    pub(in crate::repo) fn apply_picked(
        &mut self,
        pick: diff::Pick,
        label: &str,
        saved: Result<String, String>,
        ctx: &egui::Context,
    ) {
        let patch = match saved {
            Ok(p) => p,
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((e, true)));
                return;
            }
        };
        let mut args = pick.args().to_vec();
        args.push(&patch);
        let kind = if pick == diff::Pick::Discard {
            // Applying the same patch forward brings the lines back.
            let undo = Undo::Custom {
                undo: ops(&[&["apply", &patch]]),
                redo: ops(&[&args]),
            };
            OpKind::Normal(undo, self.refs.head.clone())
        } else {
            OpKind::plain()
        };
        self.run_prepared(
            label,
            &crate::i18n::message(
                "repo-core-0169",
                &[(
                    "label",
                    crate::repo::messages::display_operation(label).into(),
                )],
            ),
            ops(&[&args]),
            Refresh::Status,
            kind,
            ctx,
        );
    }
}
