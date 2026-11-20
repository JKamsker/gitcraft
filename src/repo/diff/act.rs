//! What the open diff is asked: how it shows, and moving around in it. The root
//! [`Action`](crate::repo::Action) routes these here in one arm.

use super::coverage::CoverageAct;
use super::encrypted::JsonEncoding;
use super::rich::RichKind;
use crate::repo::RepoTab;
use crate::repo::actions::PaletteContribution;

pub(in crate::repo) enum DiffAct {
    /// How diffs show: side by side, without whitespace changes, the whole file.
    Options {
        split: bool,
        ignore_ws: bool,
        whole_file: bool,
    },
    /// The diff algorithm for the open diff's file type (`None`: git's default), kept for
    /// that type; the diff is read again where it was scrolled to.
    Algorithm(Option<crate::git::DiffAlgorithm>),
    /// The open diff of a folded file (a lockfile…) shows its lines.
    ShowFolded,
    /// Ask git what `.gitattributes` says about folding these files.
    FoldAttributes(Vec<String>),
    /// Diffs show the whole file as it is after the change, or the changes.
    AsFile(bool),
    /// Markdown files show rendered in diffs, or as text.
    Rendered(bool),
    BesideGraph(bool),
    /// Diffs show as this rich view (prose…), for the files it reads, or as their lines.
    Rich(Option<RichKind>),
    /// Back from a rich view to the plain diff, at the line numbered `old` before the change
    /// or `new` after it.
    GoToLine {
        old: Option<u32>,
        new: Option<u32>,
    },
    /// Read syntax changes through Difftastic; staging stays in the patch.
    Structural(bool),
    Decrypted(bool),
    SvgSource(bool),
    Wrap(bool),
    Coverage(CoverageAct),
    DecryptedEncoding(JsonEncoding),
    /// Scroll the diff to the next change (`1`) or the previous one (`-1`).
    Step(i32),
    /// Show the two places of a moved block of the diff together (by its index), or stop.
    Moved(Option<usize>),
    /// Scroll the diff to one of its lines (by its index).
    JumpLine(usize),
    /// Show some of the unchanged lines folded in a gap of the diff (by its index).
    Unfold {
        gap: usize,
        how: super::Unfold,
    },
}

/// The palette's entry for the coverage gutter, at its slot after the file comparisons.
pub(in crate::repo) fn coverage_palette_item() -> PaletteContribution {
    (
        crate::i18n::text("diff-coverage-toggle").to_owned(),
        "coverage lcov cobertura gutter tests".to_owned(),
        crate::repo::Action::Diff(DiffAct::Coverage(CoverageAct::Toggle)),
    )
}

impl RepoTab {
    pub(in crate::repo) fn diff_act(&mut self, act: DiffAct, ctx: &egui::Context) {
        match act {
            DiffAct::Options {
                split,
                ignore_ws,
                whole_file,
            } => self.set_diff_options(split, ignore_ws, whole_file, ctx),
            DiffAct::Algorithm(algorithm) => self.set_diff_algorithm(algorithm, ctx),
            DiffAct::ShowFolded => {
                if let Some(view) = &mut self.diff {
                    view.unfolded = true;
                }
            }
            DiffAct::FoldAttributes(paths) => self.ask_fold_attributes(&paths, ctx),
            DiffAct::AsFile(on) => self.show_diff_as_file(on, ctx),
            DiffAct::Rendered(on) => self.show_diff_rendered(on, ctx),
            DiffAct::BesideGraph(on) => self.set_diff_beside_graph(on, ctx),
            DiffAct::Rich(kind) => self.show_diff_rich(kind, ctx),
            DiffAct::GoToLine { old, new } => self.diff_go_to_line(old, new),
            DiffAct::Wrap(on) => self.wrap_diff(on, ctx),
            DiffAct::Coverage(act) => self.diff_coverage(act, ctx),
            DiffAct::SvgSource(on) => {
                if let Some(view) = &mut self.diff {
                    view.svg_source = on;
                }
            }
            DiffAct::Decrypted(on) => self.show_diff_decrypted(on, ctx),
            DiffAct::DecryptedEncoding(encoding) => {
                if let Some(view) = &mut self.diff {
                    view.encrypted.close();
                    view.encrypted.json_encoding = encoding;
                }
            }
            DiffAct::Structural(on) => self.show_diff_structural(on, ctx),
            DiffAct::Step(delta) => self.step_displayed_diff(delta, ctx),
            DiffAct::Moved(block) => self.show_moved(block),
            DiffAct::Unfold { gap, how } => {
                if let Some(view) = &mut self.diff {
                    view.unfold(gap, how);
                }
            }
            DiffAct::JumpLine(i) => {
                if let Some(view) = &mut self.diff {
                    view.jump_line = Some(i);
                }
            }
        }
    }

    fn show_moved(&mut self, block: Option<usize>) {
        if let Some(view) = &mut self.diff {
            view.moved = match (&view.data, block) {
                (Some(Ok(data)), Some(b)) => super::moved_view::Opened::new(data, b),
                _ => None,
            };
        }
    }
}
