//! The ignore-and-attributes section's state and background work: explaining a path, the
//! ignored files, the files of rules, and editing one with a preview before saving.

use super::{Read, Section};
use crate::git::doctor::ignore::{self, Explained};
use crate::git::doctor::rules::{self, Preview, RulesFile};
use crate::repo::RepoTab;

/// Most ignored files listed.
pub(super) const IGNORED_SHOWN: usize = 500;

/// What the ignore section asks for.
pub(crate) enum IgnoreAct {
    /// Explain this path (as typed, or from a file's menu).
    Explain(String),
    /// Open the file of rules at this index of the list in the editor.
    Edit(usize),
    CloseEditor,
    /// What the edited text would ignore, by that file's rules alone.
    Preview,
    Save,
}

/// The rules file being edited.
pub(crate) struct Editing {
    pub(super) file: RulesFile,
    /// Its text as read, which saving checks is still what is on disk.
    pub(super) original: Read<String>,
    pub(super) text: String,
    pub(super) preview: Option<Read<Preview>>,
    pub(super) saving: bool,
    /// How the last save went.
    pub(super) saved: Option<Result<(), String>>,
    /// Scroll the page to it once it shows.
    pub(super) scroll_to: bool,
}

/// What the section shows.
#[derive(Default)]
pub(crate) struct IgnoreState {
    /// The path typed into the explain field.
    pub(super) typed: String,
    /// The path explained, and what was found.
    pub(super) explained: Option<(String, Read<Explained>)>,
    /// The ignored files (at most [`IGNORED_SHOWN`]) and how many there are.
    pub(super) ignored: Read<(Vec<String>, usize)>,
    pub(super) files: Read<Vec<RulesFile>>,
    pub(super) editing: Option<Editing>,
}

impl RepoTab {
    pub(super) fn ignore_act(&mut self, act: IgnoreAct, ctx: &egui::Context) {
        match act {
            IgnoreAct::Explain(path) => self.explain_path(path, ctx),
            IgnoreAct::Edit(index) => self.edit_rules(index, ctx),
            IgnoreAct::CloseEditor => {
                if let Some(d) = &mut self.doctor {
                    d.ignore.editing = None;
                }
            }
            IgnoreAct::Preview => self.preview_rules(ctx),
            IgnoreAct::Save => self.save_rules(ctx),
        }
    }

    /// Reads the ignored files and the files of rules (the section's own reading).
    pub(super) fn load_ignore(&mut self, read: u64, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(d) = &mut self.doctor else {
            return;
        };
        d.ignore.ignored = None;
        d.ignore.files = None;
        let again = d.ignore.explained.as_ref().map(|(p, _)| p.clone());
        self.then(
            ctx,
            move || {
                (
                    ignore::ignored_files(&path, IGNORED_SHOWN),
                    rules::files(&path),
                )
            },
            move |tab, (ignored, files), _| {
                if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                    d.ignore.ignored = Some(ignored);
                    d.ignore.files = Some(files);
                }
            },
        );
        // What was explained may read differently now.
        if let Some(path) = again {
            self.explain_path(path, ctx);
        }
    }

    /// Opens the section (if it isn't shown) and explains `path` there.
    fn explain_path(&mut self, path: String, ctx: &egui::Context) {
        if self
            .doctor
            .as_ref()
            .is_none_or(|d| d.section != Section::Ignore)
        {
            self.open_doctor(Section::Ignore, ctx);
        }
        let repo = self.path.clone();
        let Some(d) = &mut self.doctor else {
            return;
        };
        d.ignore.typed = path.clone();
        d.ignore.explained = Some((path.clone(), None));
        let asked = path.clone();
        self.then(
            ctx,
            move || ignore::explain(&repo, &asked),
            move |tab, found, _| {
                if let Some(d) = &mut tab.doctor
                    && let Some((shown, slot)) = &mut d.ignore.explained
                    && *shown == path
                {
                    *slot = Some(found);
                }
            },
        );
    }

    fn edit_rules(&mut self, index: usize, ctx: &egui::Context) {
        let Some(d) = &mut self.doctor else {
            return;
        };
        let Some(file) = d
            .ignore
            .files
            .as_ref()
            .and_then(|f| f.as_ref().ok())
            .and_then(|f| f.get(index))
            .cloned()
        else {
            return;
        };
        d.ignore.editing = Some(Editing {
            file: file.clone(),
            original: None,
            text: String::new(),
            preview: None,
            saving: false,
            saved: None,
            scroll_to: true,
        });
        let label = file.label.clone();
        self.then(
            ctx,
            move || rules::read(&file),
            move |tab, found, _| {
                if let Some(e) = editing_of(tab, &label) {
                    if let Ok(text) = &found {
                        e.text = text.clone();
                    }
                    e.original = Some(found);
                }
            },
        );
    }

    fn preview_rules(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        let Some(e) = self.doctor.as_mut().and_then(|d| d.ignore.editing.as_mut()) else {
            return;
        };
        let Some(Ok(old)) = &e.original else {
            return;
        };
        let (file, old, new) = (e.file.clone(), old.clone(), e.text.clone());
        e.preview = Some(None);
        let label = file.label.clone();
        self.then(
            ctx,
            move || rules::preview(&repo, &file, &old, &new).map(|p| (new, p)),
            move |tab, found, _| {
                if let Some(e) = editing_of(tab, &label) {
                    // Only for the text it was made from.
                    e.preview = match found {
                        Ok((text, p)) if text == e.text => Some(Some(Ok(p))),
                        Ok(_) => None,
                        Err(err) => Some(Some(Err(err))),
                    };
                }
            },
        );
    }

    fn save_rules(&mut self, ctx: &egui::Context) {
        let Some(e) = self.doctor.as_mut().and_then(|d| d.ignore.editing.as_mut()) else {
            return;
        };
        let Some(Ok(old)) = &e.original else {
            return;
        };
        if e.saving {
            return;
        }
        e.saving = true;
        e.saved = None;
        let (file, old, new) = (e.file.clone(), old.clone(), e.text.clone());
        let label = file.label.clone();
        self.then(
            ctx,
            move || rules::save(&file, &old, &new).map(|()| new),
            move |tab, saved, ctx| {
                if let Some(e) = editing_of(tab, &label) {
                    e.saving = false;
                    match saved {
                        Ok(text) => {
                            e.original = Some(Ok(text));
                            e.preview = None;
                            e.saved = Some(Ok(()));
                        }
                        Err(err) => e.saved = Some(Err(err)),
                    }
                }
                // What is ignored changed: the lists, and the tab's changes.
                tab.reload_status(ctx);
                if let Some(read) = tab.doctor.as_ref().map(|d| d.read) {
                    tab.load_ignore(read, ctx);
                }
            },
        );
    }
}

/// The editor, while it still edits the file `label`.
fn editing_of<'a>(tab: &'a mut RepoTab, label: &str) -> Option<&'a mut Editing> {
    tab.doctor
        .as_mut()?
        .ignore
        .editing
        .as_mut()
        .filter(|e| e.file.label == label)
}
