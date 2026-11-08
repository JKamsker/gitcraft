//! The order the graph lists commits in, and whether it follows only first parents.

use serde::{Deserialize, Serialize};

/// How commits that don't descend from one another are ordered. Each keeps every parent after
/// all of its children, which the graph's lanes rely on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    /// `--topo-order`: a line of history is shown together rather than interleaved.
    Topo,
    /// `--date-order`: by commit date.
    #[default]
    Date,
    /// `--author-date-order`: by author date (when the change was written, not applied).
    AuthorDate,
}

impl Order {
    pub const ALL: [Order; 3] = [Order::Topo, Order::Date, Order::AuthorDate];

    pub fn label(self) -> &'static str {
        match self {
            Order::Topo => crate::i18n::text("backend-label-33842988d845"),
            Order::Date => crate::i18n::text("backend-commit-date-default-9a50c8"),
            Order::AuthorDate => crate::i18n::text("backend-author-date-1556fe"),
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            Order::Topo => crate::i18n::text(
                "backend-keeps-each-line-of-history-together-git-log-topo-order-064ffb",
            ),
            Order::Date => {
                crate::i18n::text("backend-newest-commit-date-first-git-log-date-order-a6f7b4")
            }
            Order::AuthorDate => crate::i18n::text(
                "backend-newest-author-date-first-git-log-author-date-order-60d723",
            ),
        }
    }

    fn arg(self) -> &'static str {
        match self {
            Order::Topo => "--topo-order",
            Order::Date => "--date-order",
            Order::AuthorDate => "--author-date-order",
        }
    }
}

/// How the graph's history is read: its [`Order`], and whether only first parents are
/// followed (a merge shows, the commits it brought in don't).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogOrder {
    pub order: Order,
    pub first_parent: bool,
}

impl LogOrder {
    /// The options of `git log` that read history so.
    pub fn args(self) -> Vec<&'static str> {
        let mut args = vec![self.order.arg()];
        if self.first_parent {
            args.push("--first-parent");
        }
        args
    }

    /// Whether it reads history as git does by default here (by commit date, every parent).
    pub fn is_default(self) -> bool {
        self == Self::default()
    }

    /// In short, for the graph's header: `None` when it is the default.
    pub fn summary(self) -> Option<String> {
        if self.is_default() {
            return None;
        }
        let order = match self.order {
            Order::Topo => Some(crate::i18n::text("backend-label-5efb87c8b031")),
            Order::Date => None,
            Order::AuthorDate => Some(crate::i18n::text("backend-label-27d7acf0ec1e")),
        };
        let first = self
            .first_parent
            .then_some(crate::i18n::text("backend-label-7d0e10c239aa"));
        Some(
            order
                .into_iter()
                .chain(first)
                .collect::<Vec<_>>()
                .join(" · "),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_order_has_its_option_and_first_parent_adds_one() {
        let topo = LogOrder {
            order: Order::Topo,
            first_parent: true,
        };
        assert_eq!(topo.args(), ["--topo-order", "--first-parent"]);
        assert_eq!(LogOrder::default().args(), ["--date-order"]);
        let author = LogOrder {
            order: Order::AuthorDate,
            first_parent: false,
        };
        assert_eq!(author.args(), ["--author-date-order"]);
    }

    #[test]
    fn the_summary_names_only_what_differs_from_the_default() {
        assert_eq!(LogOrder::default().summary(), None);
        let first = LogOrder {
            first_parent: true,
            ..LogOrder::default()
        };
        assert_eq!(first.summary().as_deref(), Some("first parent"));
        let both = LogOrder {
            order: Order::Topo,
            first_parent: true,
        };
        assert_eq!(both.summary().as_deref(), Some("topo · first parent"));
    }

    /// What was kept by an older or newer gitcraft still reads: a missing field is its default.
    #[test]
    fn a_kept_order_reads_back_with_missing_fields_defaulted() {
        let kept: LogOrder = serde_json::from_str(r#"{"order":"AuthorDate"}"#).unwrap_or_default();
        assert_eq!(kept.order, Order::AuthorDate);
        assert!(!kept.first_parent);
        let round: LogOrder =
            serde_json::from_str(&serde_json::to_string(&kept).unwrap_or_default())
                .unwrap_or_default();
        assert_eq!(round, kept);
    }
}
