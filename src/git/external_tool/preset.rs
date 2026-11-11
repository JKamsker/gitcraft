//! The diff and merge tools Settings offers by name: what git calls each one, how it is started
//! with the files to compare or merge, and where Windows installs it.

use serde::{Deserialize, Serialize};

/// A diff and merge tool gitcraft knows how to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    BeyondCompare,
    Araxis,
    VsCode,
    Meld,
    KDiff3,
    P4Merge,
    WinMerge,
}

/// One word of a tool's command line: as it is, or one of the files git hands over
/// (`$LOCAL`, `$REMOTE`, `$BASE`, `$MERGED`), maybe glued to an option (`--output=`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    Text(&'static str),
    File(&'static str, Slot),
}

/// A file git hands a tool, by the variable it is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Local,
    Remote,
    Base,
    Merged,
}

impl Slot {
    /// The shell variable git sets to it.
    pub fn var(self) -> &'static str {
        match self {
            Slot::Local => "LOCAL",
            Slot::Remote => "REMOTE",
            Slot::Base => "BASE",
            Slot::Merged => "MERGED",
        }
    }
}

use Slot::{Base, Local, Merged, Remote};
use Word::{File, Text};

impl Preset {
    pub const ALL: [Preset; 7] = [
        Preset::BeyondCompare,
        Preset::Araxis,
        Preset::VsCode,
        Preset::Meld,
        Preset::KDiff3,
        Preset::P4Merge,
        Preset::WinMerge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::BeyondCompare => "Beyond Compare",
            Preset::Araxis => "Araxis Merge",
            Preset::VsCode => "VS Code",
            Preset::Meld => "Meld",
            Preset::KDiff3 => "KDiff3",
            Preset::P4Merge => "P4Merge",
            Preset::WinMerge => "WinMerge",
        }
    }

    /// A short name of its own, for the tool gitcraft defines for it (`gitcraft-vscode`).
    pub fn id(self) -> &'static str {
        match self {
            Preset::BeyondCompare => "bc",
            Preset::Araxis => "araxis",
            Preset::VsCode => "vscode",
            Preset::Meld => "meld",
            Preset::KDiff3 => "kdiff3",
            Preset::P4Merge => "p4merge",
            Preset::WinMerge => "winmerge",
        }
    }

    /// The name git itself knows it by (`git difftool --tool=bc`), when git has it built in.
    /// VS Code isn't among git's own tools in the versions most people have.
    pub fn git_name(self) -> Option<&'static str> {
        match self {
            Preset::VsCode => None,
            other => Some(other.id()),
        }
    }

    /// The program run when none is set and git doesn't find it by itself: the one on the
    /// PATH.
    pub fn default_program(self) -> &'static str {
        match self {
            Preset::BeyondCompare => "bcompare",
            Preset::Araxis => "compare",
            Preset::VsCode => "code",
            Preset::Meld => "meld",
            Preset::KDiff3 => "kdiff3",
            Preset::P4Merge => "p4merge",
            Preset::WinMerge => "WinMergeU",
        }
    }

    /// What follows the program to show `$LOCAL` against `$REMOTE`, as git's own tool
    /// definitions do it.
    pub fn diff_words(self) -> &'static [Word] {
        match self {
            Preset::BeyondCompare | Preset::Meld | Preset::KDiff3 | Preset::P4Merge => {
                &[File("", Local), File("", Remote)]
            }
            Preset::Araxis => &[Text("-wait"), Text("-2"), File("", Local), File("", Remote)],
            Preset::VsCode => &[
                Text("--wait"),
                Text("--diff"),
                File("", Local),
                File("", Remote),
            ],
            Preset::WinMerge => &[Text("-u"), Text("-e"), File("", Local), File("", Remote)],
        }
    }

    /// What follows the program to merge `$LOCAL` and `$REMOTE` from `$BASE` into `$MERGED`.
    pub fn merge_words(self) -> &'static [Word] {
        match self {
            Preset::BeyondCompare => &[
                File("", Local),
                File("", Remote),
                File("", Base),
                File("-mergeoutput=", Merged),
            ],
            Preset::Araxis => &[
                Text("-wait"),
                Text("-merge"),
                Text("-3"),
                Text("-a1"),
                File("", Base),
                File("", Local),
                File("", Remote),
                File("", Merged),
            ],
            Preset::VsCode => &[
                Text("--wait"),
                Text("--merge"),
                File("", Remote),
                File("", Local),
                File("", Base),
                File("", Merged),
            ],
            Preset::Meld => &[
                File("--output=", Merged),
                File("", Local),
                File("", Base),
                File("", Remote),
            ],
            Preset::KDiff3 => &[
                Text("--auto"),
                File("", Base),
                File("", Local),
                File("", Remote),
                Text("-o"),
                File("", Merged),
            ],
            Preset::P4Merge => &[
                File("", Base),
                File("", Remote),
                File("", Local),
                File("", Merged),
            ],
            Preset::WinMerge => &[
                Text("-u"),
                Text("-e"),
                Text("-dl"),
                Text("Local"),
                Text("-dr"),
                Text("Remote"),
                File("", Local),
                File("", Remote),
                File("", Merged),
            ],
        }
    }

    /// Whether its exit code says if the merge worked, when `mergetool.<tool>.trustExitCode`
    /// doesn't say: git's own answer for each (only KDiff3's is trusted). Otherwise git looks at
    /// whether the merged file was saved.
    pub fn trusts_exit_code(self) -> bool {
        matches!(self, Preset::KDiff3)
    }

    /// Where its Windows installer puts it: (the folder variable, the path under it). Beyond
    /// Compare's `BComp.exe`, not `BCompare.exe`, which hands the files to a window already
    /// open and returns at once (git would take the merge for unsaved).
    pub fn windows_places(self) -> &'static [(&'static str, &'static str)] {
        const PF: &str = "ProgramFiles";
        const PF86: &str = "ProgramFiles(x86)";
        const LOCAL: &str = "LOCALAPPDATA";
        match self {
            Preset::BeyondCompare => &[
                (PF, r"Beyond Compare 5\BComp.exe"),
                (PF, r"Beyond Compare 4\BComp.exe"),
                (PF86, r"Beyond Compare 4\BComp.exe"),
                (PF, r"Beyond Compare 3\BComp.exe"),
                (PF86, r"Beyond Compare 3\BComp.exe"),
                (LOCAL, r"Programs\Beyond Compare 5\BComp.exe"),
                (LOCAL, r"Programs\Beyond Compare 4\BComp.exe"),
            ],
            Preset::Araxis => &[
                (PF, r"Araxis\Araxis Merge\Compare.exe"),
                (PF86, r"Araxis\Araxis Merge\Compare.exe"),
            ],
            Preset::VsCode => &[
                (LOCAL, r"Programs\Microsoft VS Code\Code.exe"),
                (PF, r"Microsoft VS Code\Code.exe"),
            ],
            Preset::Meld => &[
                (PF, r"Meld\Meld.exe"),
                (PF86, r"Meld\Meld.exe"),
                (LOCAL, r"Programs\Meld\Meld.exe"),
            ],
            Preset::KDiff3 => &[
                (PF, r"KDiff3\kdiff3.exe"),
                (PF, r"KDiff3\bin\kdiff3.exe"),
                (PF86, r"KDiff3\kdiff3.exe"),
            ],
            Preset::P4Merge => &[
                (PF, r"Perforce\p4merge.exe"),
                (PF86, r"Perforce\p4merge.exe"),
            ],
            Preset::WinMerge => &[
                (PF, r"WinMerge\WinMergeU.exe"),
                (PF86, r"WinMerge\WinMergeU.exe"),
                (LOCAL, r"Programs\WinMerge\WinMergeU.exe"),
            ],
        }
    }
}

/// Which diff or merge tool to use: git's own setting, or one of the presets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolChoice {
    /// `None`: what git's config names (`diff.tool` / `merge.tool`).
    pub preset: Option<Preset>,
    /// The preset's program, when it isn't where it is looked for (empty: found by itself).
    pub program: String,
}

impl ToolChoice {
    /// What the choice is called, for a message: "Beyond Compare", or what git is set to use.
    pub fn label(&self, kind: Kind) -> String {
        match self.preset {
            Some(p) => p.label().to_owned(),
            None => crate::i18n::message(
                "backend-the-tool-set-in-git-05b37c",
                &[
                    ("kind", (kind.noun()).to_string().into()),
                    ("kind2", (kind.key()).to_string().into()),
                ],
            ),
        }
    }
}

/// Showing a diff, or merging a conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Diff,
    Merge,
}

impl Kind {
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Diff => "diff",
            Kind::Merge => "merge",
        }
    }

    /// The git command that runs the tool.
    pub fn command(self) -> &'static str {
        match self {
            Kind::Diff => "difftool",
            Kind::Merge => "mergetool",
        }
    }

    /// The config key naming the tool.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Diff => "diff.tool",
            Kind::Merge => "merge.tool",
        }
    }
}
