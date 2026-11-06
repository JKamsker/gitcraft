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
            Operator::Author => "commits by this author (name or email)",
            Operator::Committer => "commits committed by this person (name or email)",
            Operator::Path => "commits that changed this file or folder",
            Operator::After => "commits since a date: 2024-05-01, 2w, 3m, today…",
            Operator::Before => "commits before a date: 2024-05-01, 1y, yesterday…",
            Operator::Message => "commits whose message has this text",
            Operator::Contents => "commits that added or removed this text (git log -S)",
            Operator::Regex => "commits whose changes match this regex (git log -G)",
            Operator::Merges => "only merge commits; -merges: leaves them out",
        }
    }

    /// What its value is, for the hint shown when it has none.
    pub fn needs(self) -> &'static str {
        match self {
            Operator::Author | Operator::Committer => "a name or an email",
            Operator::Path => "a file or a folder",
            Operator::After | Operator::Before => "a date such as 2024-05-01 or 2w",
            Operator::Message | Operator::Contents => "some text",
            Operator::Regex => "a regular expression",
            Operator::Merges => "no value",
        }
    }

    /// Whether `-name:` means something (leaving out what it would find).
    pub fn negatable(self) -> bool {
        self == Operator::Merges
    }
}
