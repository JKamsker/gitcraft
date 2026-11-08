//! The stash dialog: everything, the files picked, only what is staged, or a snapshot that
//! keeps every change where it is; with a message and whether untracked files go too.

use egui::{RichText, Ui};

use crate::git::WorkStatus;
use crate::repo::Action;
use crate::repo::partial_stash::{
    StagedSupport, StashMode, StashRequest, everything_problem, files_problem, has_untracked,
};
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title};

/// What the dialog stashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Everything,
    Files,
    Staged,
    Snapshot,
}

pub struct StashDialog {
    pub(super) choice: Choice,
    /// The files to stash with [`Choice::Files`]: those picked, or the one whose menu opened it.
    pub(super) files: Vec<String>,
    pub(super) message: String,
    pub(super) untracked: bool,
    pub(super) focus: bool,
    /// The files picked in the changes were taken in (on the first frame).
    pub(super) seeded: bool,
    /// Opened: a git that couldn't be asked whether it stashes only what is staged is asked
    /// again.
    pub(super) opened: bool,
}

impl StashDialog {
    pub(super) fn new(choice: Choice, files: Vec<String>) -> Self {
        StashDialog {
            choice,
            seeded: !files.is_empty(),
            files,
            message: String::new(),
            untracked: true,
            focus: true,
            opened: false,
        }
    }

    /// Takes in the files picked in the changes, once: opened from the toolbar or the palette
    /// with files picked, it offers (and starts with) stashing them.
    pub(super) fn seed(&mut self, picked: impl IntoIterator<Item = String>) {
        if self.seeded {
            return;
        }
        self.seeded = true;
        self.files = picked.into_iter().collect();
        if !self.files.is_empty() && self.choice == Choice::Everything {
            self.choice = Choice::Files;
        }
    }

    /// Lets go of the files that have no changes any more (committed, discarded, stashed
    /// elsewhere while it is open); with none left, it stashes everything instead.
    pub(super) fn keep_changed(&mut self, work: &WorkStatus) {
        if self.files.is_empty() {
            return;
        }
        crate::repo::partial_stash::keep_changed_files(&mut self.files, work);
        if self.files.is_empty() && self.choice == Choice::Files {
            self.choice = Choice::Everything;
        }
    }

    /// The stash the dialog asks for as it is filled in.
    pub(crate) fn request(&self) -> StashRequest {
        let (mode, untracked) = match self.choice {
            Choice::Everything => (StashMode::Everything, self.untracked),
            Choice::Files => (StashMode::Files(self.files.clone()), false),
            Choice::Staged => (StashMode::Staged, false),
            Choice::Snapshot => (StashMode::Snapshot, self.untracked),
        };
        StashRequest {
            mode,
            message: self.message.trim().to_owned(),
            untracked,
        }
    }
}

/// Said while the repository's git is asked whether it can stash only what is staged.
const CHECKING: &str = "repo-core-1489";

/// `text` as a sentence of its own: messages start in lower case, as they read after
/// "<operation> failed: " in the status line.
pub(crate) fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Why stashing only what is staged can't be done now, if it can't.
pub(super) fn staged_problem(work: &WorkStatus, support: &StagedSupport) -> Option<String> {
    if work.staged.is_empty() {
        return Some(crate::i18n::text("repo-core-1490").into());
    }
    match support {
        StagedSupport::Known(Ok(())) => None,
        StagedSupport::Known(Err(e)) | StagedSupport::Failed(e) => Some(e.clone()),
        StagedSupport::Unknown | StagedSupport::Checking => {
            Some(crate::i18n::text(CHECKING).into())
        }
    }
}

/// Why the dialog's choice can't be stashed, if it can't.
pub(super) fn problem(
    d: &StashDialog,
    work: &WorkStatus,
    support: &StagedSupport,
) -> Option<String> {
    match d.choice {
        Choice::Everything | Choice::Snapshot => everything_problem(work, d.untracked),
        Choice::Files => files_problem(&d.files, work),
        Choice::Staged => staged_problem(work, support),
    }
}

/// `a.txt, b.txt, c.txt and 4 more`.
pub(super) fn files_summary(files: &[String]) -> String {
    const SHOWN: usize = 3;
    let names: Vec<&str> = files.iter().take(SHOWN).map(String::as_str).collect();
    let names = names.join(", ");
    match files.len().saturating_sub(SHOWN) {
        0 => names,
        more => crate::i18n::message(
            "repo-core-1491",
            &[
                ("names", format!("{}", names).into()),
                ("more", format!("{}", more).into()),
            ],
        ),
    }
}

pub(super) fn stash(
    ui: &mut Ui,
    enter: bool,
    d: &mut StashDialog,
    work: &WorkStatus,
    support: &StagedSupport,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1492"),
        crate::i18n::text("repo-core-1493"),
        &t,
    );
    choice(
        ui,
        &mut d.choice,
        Choice::Everything,
        crate::i18n::text("repo-core-1494"),
        crate::i18n::text("repo-core-1495"),
        None,
        &t,
    );
    if d.files.is_empty() {
        let how = crate::i18n::message(
            "repo-core-1496",
            &[("command", format!("{}", crate::manual::command()).into())],
        );
        choice(
            ui,
            &mut d.choice,
            Choice::Files,
            crate::i18n::text("repo-core-1497"),
            "",
            Some(&how),
            &t,
        );
    } else {
        let n = d.files.len();
        let label = crate::i18n::message(
            "repo-core-1498",
            &[
                ("n", (n).into()),
                ("s", format!("{}", if n == 1 { "" } else { "s" }).into()),
            ],
        );
        let hint = files_summary(&d.files);
        choice(ui, &mut d.choice, Choice::Files, &label, &hint, None, &t);
    }
    let staged = staged_problem(work, support);
    choice(
        ui,
        &mut d.choice,
        Choice::Staged,
        crate::i18n::text("repo-core-1499"),
        crate::i18n::text("repo-core-1500"),
        staged.as_deref(),
        &t,
    );
    choice(
        ui,
        &mut d.choice,
        Choice::Snapshot,
        crate::i18n::text("repo-core-1501"),
        crate::i18n::text("repo-core-1502"),
        None,
        &t,
    );
    ui.add_space(8.0);
    field(
        ui,
        &mut d.message,
        crate::i18n::text("repo-core-1503"),
        &mut d.focus,
    );
    ui.add_space(6.0);
    untracked_box(ui, d, work);
    let problem = problem(d, work, support);
    if let Some(p) = &problem
        && d.choice != Choice::Staged
    {
        ui.label(RichText::new(sentence(p)).color(t.danger).small());
    }
    ui.add_space(10.0);
    let button = if d.choice == Choice::Snapshot {
        crate::i18n::text("repo-core-1504")
    } else {
        crate::i18n::text("repo-core-0862")
    };
    let request = d.request();
    buttons(ui, button, problem.is_none(), enter, || {
        Action::Stash(request)
    })
}

/// One of the choices, with what it does under it (or, when it can't be chosen, why not).
fn choice(
    ui: &mut Ui,
    current: &mut Choice,
    value: Choice,
    label: &str,
    hint: &str,
    unavailable: Option<&str>,
    t: &Tokens,
) {
    ui.add_enabled_ui(unavailable.is_none() || *current == value, |ui| {
        ui.radio_value(current, value, label);
    });
    let (text, color) = match unavailable {
        // Not a warning while it is only being found out.
        Some(why) if why == crate::i18n::text(CHECKING) => (why, t.text_faint),
        Some(why) => (why, t.warning),
        None => (hint, t.text_faint),
    };
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(
            RichText::new(sentence(text))
                .color(color)
                .font(egui::FontId::proportional(11.5)),
        );
    });
    ui.add_space(2.0);
}

/// Include untracked files: a choice for everything and snapshots; picked files go as they
/// are picked; git stashes none with only what is staged.
fn untracked_box(ui: &mut Ui, d: &mut StashDialog, work: &WorkStatus) {
    match d.choice {
        Choice::Everything | Choice::Snapshot => {
            ui.checkbox(&mut d.untracked, crate::i18n::text("repo-core-1505"));
        }
        Choice::Files => {
            let mut picked = has_untracked(&d.files, work);
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut picked, crate::i18n::text("repo-core-1505")),
            )
            .on_disabled_hover_text(crate::i18n::text("repo-core-1506"));
        }
        Choice::Staged => {
            let mut none = false;
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut none, crate::i18n::text("repo-core-1505")),
            )
            .on_disabled_hover_text(crate::i18n::text("repo-core-1507"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Area, StatusFile};

    fn file(path: &str, code: char, area: Area) -> StatusFile {
        StatusFile {
            path: path.to_owned(),
            old_path: None,
            code,
            area,
        }
    }

    #[test]
    fn long_lists_of_files_are_summed_up() {
        let files = |n: usize| (0..n).map(|i| format!("f{i}")).collect::<Vec<_>>();
        assert_eq!(files_summary(&files(1)), "f0");
        assert_eq!(files_summary(&files(3)), "f0, f1, f2");
        assert_eq!(files_summary(&files(5)), "f0, f1, f2 and 2 more");
    }

    #[test]
    fn picked_files_are_offered_once_and_chosen() {
        let mut d = StashDialog::new(Choice::Everything, Vec::new());
        d.seed(["a b.txt".to_owned()]);
        assert_eq!(d.choice, Choice::Files);
        assert_eq!(d.files, ["a b.txt"]);
        // Picked later, while it is open: it keeps what it has.
        d.seed(["c".to_owned()]);
        assert_eq!(d.files, ["a b.txt"]);
        // From a file's menu: that file, whatever is picked.
        let mut d = StashDialog::new(Choice::Files, vec!["x".to_owned()]);
        d.seed(["y".to_owned()]);
        assert_eq!(d.files, ["x"]);
        // Asked for only what is staged: it stays that.
        let mut d = StashDialog::new(Choice::Staged, Vec::new());
        d.seed(["y".to_owned()]);
        assert_eq!(d.choice, Choice::Staged);
    }

    #[test]
    fn the_request_takes_untracked_files_only_where_they_can_go() {
        let mut d = StashDialog::new(Choice::Everything, vec!["a".to_owned()]);
        d.message = "  wip  ".to_owned();
        assert_eq!(d.request(), StashRequest::everything("wip"));
        d.untracked = false;
        assert!(!d.request().untracked);
        d.choice = Choice::Snapshot;
        d.untracked = true;
        assert_eq!(d.request().mode, StashMode::Snapshot);
        assert!(d.request().untracked);
        d.choice = Choice::Staged;
        assert!(!d.request().untracked);
        d.choice = Choice::Files;
        assert_eq!(d.request().mode, StashMode::Files(vec!["a".to_owned()]));
    }

    #[test]
    fn what_cant_be_stashed_says_why() {
        let clean = WorkStatus::default();
        let known = StagedSupport::Known(Ok(()));
        let d = StashDialog::new(Choice::Snapshot, Vec::new());
        assert!(problem(&d, &clean, &known).is_some());
        let work = WorkStatus {
            unstaged: vec![file("a", 'M', Area::Unstaged)],
            ..WorkStatus::default()
        };
        assert_eq!(problem(&d, &work, &known), None);
        let mut d = StashDialog::new(Choice::Staged, Vec::new());
        assert_eq!(
            problem(&d, &work, &known).as_deref(),
            Some("nothing is staged")
        );
        let staged = WorkStatus {
            staged: vec![file("a", 'M', Area::Staged)],
            ..WorkStatus::default()
        };
        assert_eq!(problem(&d, &staged, &known), None);
        assert!(problem(&d, &staged, &StagedSupport::Checking).is_some());
        let old = StagedSupport::Known(Err("needs git 2.35 or newer".into()));
        assert_eq!(
            problem(&d, &staged, &old).as_deref(),
            Some("needs git 2.35 or newer")
        );
        d.choice = Choice::Files;
        d.files = vec!["gone".to_owned()];
        assert!(problem(&d, &work, &known).is_some_and(|p| p.contains("no changes")));
    }
}
