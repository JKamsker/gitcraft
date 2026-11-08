//! The operators the search field understands.

/// An operator, written `name:` (or `-merges:`) before its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operator {
    Author,
    Committer,
    Path,
    After,
    Before,
    Contents,
    Regex,
    Message,
    Merges,
}

impl Operator {
    /// Every operator, in the order the suggestions list them.
    pub const ALL: [Operator; 9] = [
        Operator::Author,
        Operator::Committer,
        Operator::Path,
        Operator::After,
        Operator::Before,
        Operator::Message,
        Operator::Contents,
        Operator::Regex,
        Operator::Merges,
    ];

    /// The name written before the colon.
    pub fn name(self) -> &'static str {
        match self {
            Operator::Author => "author",
            Operator::Committer => "committer",
            Operator::Path => "path",
            Operator::After => "after",
            Operator::Before => "before",
            Operator::Contents => "contents",
            Operator::Regex => "regex",
            Operator::Message => "message",
            Operator::Merges => "merges",
        }
    }

    /// The operator called `name` (any letter case), if there is one.
    pub fn from_name(name: &str) -> Option<Operator> {
        Operator::ALL
            .into_iter()
            .find(|op| op.name().eq_ignore_ascii_case(name))
    }

    /// What it finds, for the suggestions and the field's tooltip.
    pub fn help(self) -> &'static str {
        match self {
            Operator::Author => {
                crate::i18n::text("backend-commits-by-this-author-name-or-email-4c00b8")
            }
            Operator::Committer => {
                crate::i18n::text("backend-commits-committed-by-this-person-name-or-email-411d27")
            }
            Operator::Path => {
                crate::i18n::text("backend-commits-that-changed-this-file-or-folder-a39810")
            }
            Operator::After => {
                crate::i18n::text("backend-commits-since-a-date-2024-05-01-2w-3m-today-c10ca1")
            }
            Operator::Before => {
                crate::i18n::text("backend-commits-before-a-date-2024-05-01-1y-yesterday-342e7e")
            }
            Operator::Message => {
                crate::i18n::text("backend-commits-whose-message-has-this-text-cc8a3e")
            }
            Operator::Contents => crate::i18n::text(
                "backend-commits-that-added-or-removed-this-text-git-log-s-04ef5f",
            ),
            Operator::Regex => {
                crate::i18n::text("backend-commits-whose-changes-match-this-regex-git-log-g-3b4b81")
            }
            Operator::Merges => {
                crate::i18n::text("backend-only-merge-commits-merges-leaves-them-out-dcee25")
            }
        }
    }

    /// What its value is, for the hint shown when it has none.
    pub fn needs(self) -> &'static str {
        match self {
            Operator::Author | Operator::Committer => {
                crate::i18n::text("backend-a-name-or-an-email-2ebaba")
            }
            Operator::Path => crate::i18n::text("backend-a-file-or-a-folder-972ca3"),
            Operator::After | Operator::Before => {
                crate::i18n::text("backend-a-date-such-as-2024-05-01-or-2w-64f91a")
            }
            Operator::Message | Operator::Contents => crate::i18n::text("backend-some-text-b94f6f"),
            Operator::Regex => crate::i18n::text("backend-a-regular-expression-23ee11"),
            Operator::Merges => crate::i18n::text("backend-no-value-03b73e"),
        }
    }

    /// Whether `-name:` means something (leaving out what it would find).
    pub fn negatable(self) -> bool {
        self == Operator::Merges
    }
}
