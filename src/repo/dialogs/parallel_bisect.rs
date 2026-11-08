//! The dialog that starts a parallel bisect: its ends, the test command, the machine it runs
//! on, how many commits at once and how long one test may take.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use egui::{RichText, Ui};

use crate::parallel_bisect::{self as pb, MAX_PARALLEL, Machine, default_parallel};
use crate::repo::parallel_bisect::{ParallelAct, machines};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

use super::{Outcome, cancel_button, field, title, usable};

/// A test's time limit unless changed, in minutes.
const DEFAULT_MINUTES: u32 = 30;

/// What the dialog holds while open.
pub struct ParallelForm {
    repo: PathBuf,
    bad: String,
    /// One or more, separated by spaces.
    good: String,
    /// Commits the bisect going on skipped.
    skip: Vec<String>,
    command: String,
    machines: Vec<Machine>,
    /// The WSL distributions, while they are looked up.
    wsl: Option<Receiver<Vec<String>>>,
    machine: usize,
    /// A clone on the machine to use, when it isn't the repository's.
    repo_there: String,
    parallel: usize,
    /// The user set `parallel`: it no longer follows the machine's cores.
    parallel_set: bool,
    /// The machines' core counts, as they come in.
    cores: Arc<Mutex<HashMap<Machine, Result<usize, String>>>>,
    minutes: u32,
    focus: bool,
}

impl ParallelForm {
    /// The form for bisecting `repo` between `bad` and `good`, with what was used last time.
    pub fn new(
        repo: &Path,
        bad: String,
        good: Vec<String>,
        skip: Vec<String>,
        last: Option<&pb::Setup>,
    ) -> Self {
        let machines = machines(repo);
        let machine = last
            .and_then(|l| machines.iter().position(|m| *m == l.machine))
            .unwrap_or(0);
        let wsl = cfg!(windows).then(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::remote::wsl_distros());
            });
            rx
        });
        Self {
            repo: repo.to_path_buf(),
            bad,
            good: good.join(" "),
            skip,
            command: last.map(|l| l.command.clone()).unwrap_or_default(),
            machines,
            wsl,
            machine,
            repo_there: last.and_then(|l| l.repo_there.clone()).unwrap_or_default(),
            parallel: last.map_or(0, |l| l.parallel),
            parallel_set: last.is_some(),
            cores: Arc::default(),
            minutes: last.map_or(DEFAULT_MINUTES, |l| {
                (l.timeout.as_secs() / 60).clamp(1, u64::from(u32::MAX)) as u32
            }),
            focus: true,
        }
    }

    fn chosen(&self) -> &Machine {
        self.machines.get(self.machine).unwrap_or(&Machine::Here)
    }

    /// Whether the chosen machine is the repository's own.
    fn at_home(&self) -> bool {
        *self.chosen() == Machine::of(&self.repo)
    }

    /// Takes in the WSL distributions once found.
    fn take_wsl(&mut self) {
        let Some(rx) = &self.wsl else {
            return;
        };
        let Ok(distros) = rx.try_recv() else {
            return;
        };
        self.wsl = None;
        for distro in distros {
            let m = Machine::There(crate::remote::Host::Wsl(distro));
            if !self.machines.contains(&m) {
                self.machines.push(m);
            }
        }
    }

    /// The chosen machine's cores, asked for in the background the first time.
    fn cores(&mut self, ctx: &egui::Context) -> Option<Result<usize, String>> {
        let machine = self.chosen().clone();
        let mut known = self.cores.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = known.get(&machine) {
            return Some(found.clone());
        }
        known.insert(machine.clone(), Err(String::new()));
        drop(known);
        let (cores, ctx) = (Arc::clone(&self.cores), ctx.clone());
        std::thread::spawn(move || {
            let found = machine.cores();
            cores
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(machine, found);
            ctx.request_repaint();
        });
        None
    }

    /// What is still missing before it can start, said as what to do.
    fn missing(&self) -> &'static str {
        let good: Vec<&str> = self.good.split_whitespace().collect();
        if !usable(&self.bad) {
            crate::i18n::text("repo-core-1403")
        } else if good.is_empty() {
            crate::i18n::text("repo-core-1404")
        } else if !good.iter().all(|g| usable(g)) {
            crate::i18n::text("repo-core-1405")
        } else if self.command.trim().is_empty() {
            crate::i18n::text("repo-core-1406")
        } else {
            crate::i18n::text("repo-core-1407")
        }
    }

    /// The setup it gives, when it is complete.
    fn setup(&self) -> Option<pb::Setup> {
        let good: Vec<String> = self.good.split_whitespace().map(str::to_owned).collect();
        let ok = usable(&self.bad)
            && !good.is_empty()
            && good.iter().all(|g| usable(g))
            && !self.command.trim().is_empty()
            && (1..=MAX_PARALLEL).contains(&self.parallel);
        let repo_there = self.repo_there.trim();
        ok.then(|| pb::Setup {
            repo: self.repo.clone(),
            bad: self.bad.trim().to_owned(),
            good,
            skip: self.skip.clone(),
            command: self.command.trim().to_owned(),
            machine: self.chosen().clone(),
            repo_there: (!self.at_home() && !repo_there.is_empty()).then(|| repo_there.to_owned()),
            parallel: self.parallel,
            timeout: Duration::from_secs(u64::from(self.minutes.max(1)) * 60),
        })
    }
}

pub(super) fn parallel_bisect(ui: &mut Ui, enter: bool, form: &mut ParallelForm) -> Outcome {
    let t = Tokens::get(ui.ctx());
    form.take_wsl();
    title(
        ui,
        crate::i18n::text("repo-core-1408"),
        crate::i18n::text("repo-core-1409"),
        &t,
    );
    ends(ui, form, &t);
    ui.add_space(8.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1410")).color(t.text_dim));
    let mut focus = std::mem::take(&mut form.focus);
    field(
        ui,
        &mut form.command,
        "cargo test, make check, ./test.sh…",
        &mut focus,
    );
    ui.label(
        RichText::new(crate::i18n::text("repo-core-1411"))
            .small()
            .color(t.text_faint),
    );
    ui.add_space(8.0);
    machine(ui, form, &t);
    ui.add_space(8.0);
    limits(ui, form, &t);
    ui.add_space(12.0);
    let setup = form.setup();
    let missing = form.missing();
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let start = widgets::primary_button(
            ui,
            crate::i18n::text("repo-core-1207"),
            setup.is_some(),
            90.0,
        )
        .on_disabled_hover_text(missing);
        if let Some(setup) = setup
            && (start.clicked() || enter)
        {
            outcome = Outcome::Done(Box::new(Action::ParallelBisect(ParallelAct::Start(
                Box::new(setup),
            ))));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// The bad and the good commits.
fn ends(ui: &mut Ui, form: &mut ParallelForm, t: &Tokens) {
    let mut no_focus = false;
    ui.label(RichText::new(crate::i18n::text("repo-core-1412")).color(t.text_dim));
    field(
        ui,
        &mut form.bad,
        crate::i18n::text("repo-core-1204"),
        &mut no_focus,
    );
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1413")).color(t.text_dim));
    field(
        ui,
        &mut form.good,
        crate::i18n::text("repo-core-1414"),
        &mut no_focus,
    );
    if !form.skip.is_empty() {
        ui.label(
            RichText::new(match form.skip.len() {
                1 => crate::i18n::text("repo-core-1415").to_owned(),
                n => crate::i18n::message("repo-core-1416", &[("n", (n).into())]),
            })
            .small()
            .color(t.text_faint),
        );
    }
}

/// Where the tests run, and the repository there when it is another machine.
fn machine(ui: &mut Ui, form: &mut ParallelForm, t: &Tokens) {
    ui.label(RichText::new(crate::i18n::text("repo-core-1417")).color(t.text_dim));
    let home = Machine::of(&form.repo);
    let label = |m: &Machine| {
        if *m == home {
            crate::i18n::message(
                "repo-core-1418",
                &[("label", format!("{}", capital(&m.label())).into())],
            )
        } else {
            capital(&m.label())
        }
    };
    let chosen = label(form.chosen());
    egui::ComboBox::from_id_salt("parallel-bisect-machine")
        .selected_text(chosen)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (i, m) in form.machines.iter().enumerate() {
                ui.selectable_value(&mut form.machine, i, label(m));
            }
        });
    if form.wsl.is_some() {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-1420"))
                .small()
                .color(t.text_faint),
        );
    }
    if !form.at_home() {
        ui.add_space(4.0);
        ui.label(RichText::new(crate::i18n::text("repo-core-1421")).color(t.text_dim));
        let mut no_focus = false;
        field(
            ui,
            &mut form.repo_there,
            crate::i18n::text("repo-core-1422"),
            &mut no_focus,
        );
        ui.label(
            RichText::new(crate::i18n::text("repo-core-1423"))
                .small()
                .color(t.text_faint),
        );
    }
}

/// How many commits at once, and a test's time limit.
fn limits(ui: &mut Ui, form: &mut ParallelForm, t: &Tokens) {
    let cores = form.cores(ui.ctx());
    if !form.parallel_set {
        form.parallel = match &cores {
            Some(Ok(n)) => default_parallel(*n),
            _ => form.parallel.max(1),
        };
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1424")).color(t.text_dim));
        let drag = egui::DragValue::new(&mut form.parallel).range(1..=MAX_PARALLEL);
        if ui
            .add(drag)
            .on_hover_text(crate::i18n::text("repo-core-1425"))
            .changed()
        {
            form.parallel_set = true;
        }
        let (said, why) = match &cores {
            Some(Ok(n)) => (
                crate::i18n::message(
                    "repo-core-1426",
                    &[
                        ("n", (n).into()),
                        ("label", format!("{}", form.chosen().label()).into()),
                    ],
                ),
                None,
            ),
            Some(Err(e)) if !e.is_empty() => (
                crate::i18n::text("repo-core-1427").to_owned(),
                Some(e.clone()),
            ),
            _ => (crate::i18n::text("repo-core-1428").to_owned(), None),
        };
        let label = ui.label(RichText::new(said).small().color(t.text_faint));
        if let Some(why) = why {
            label.on_hover_text(why);
        }
        ui.add_space(12.0);
        ui.label(RichText::new(crate::i18n::text("repo-core-1429")).color(t.text_dim));
        ui.add(
            egui::DragValue::new(&mut form.minutes)
                .range(1..=1440)
                .suffix(crate::i18n::text("repo-core-1430")),
        )
        .on_hover_text(crate::i18n::text("repo-core-1431"));
    });
}

/// "this computer" → "This computer".
fn capital(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> ParallelForm {
        let mut form = ParallelForm::new(
            Path::new("/r"),
            "HEAD".to_owned(),
            vec!["v1".to_owned(), "v0".to_owned()],
            Vec::new(),
            None,
        );
        form.command = "make check".to_owned();
        form.parallel = 4;
        form
    }

    #[test]
    fn a_complete_form_gives_a_setup() -> Result<(), String> {
        let setup = form().setup().ok_or("no setup")?;
        assert_eq!(setup.good, ["v1", "v0"]);
        assert_eq!(setup.machine, Machine::Here);
        assert_eq!(setup.repo_there, None);
        assert_eq!(setup.timeout, Duration::from_secs(30 * 60));
        Ok(())
    }

    #[test]
    fn an_incomplete_form_gives_none() {
        for change in [
            |f: &mut ParallelForm| f.command = "  ".to_owned(),
            |f: &mut ParallelForm| f.good.clear(),
            |f: &mut ParallelForm| f.bad = "-x".to_owned(),
            |f: &mut ParallelForm| f.good = "v1 --all".to_owned(),
            |f: &mut ParallelForm| f.parallel = 0,
            |f: &mut ParallelForm| f.parallel = MAX_PARALLEL + 1,
        ] {
            let mut f = form();
            change(&mut f);
            assert!(f.setup().is_none());
        }
    }

    #[test]
    fn the_last_bisect_is_offered_again() -> Result<(), String> {
        let last = form().setup().ok_or("no setup")?;
        let again = ParallelForm::new(Path::new("/r"), "x".into(), vec![], vec![], Some(&last));
        assert_eq!(again.command, "make check");
        assert_eq!(again.parallel, 4);
        assert!(again.parallel_set);
        assert_eq!(again.minutes, 30);
        Ok(())
    }
}
