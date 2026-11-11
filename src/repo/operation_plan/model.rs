//! Lossless decoding boundary for persisted operation argv. Indices always refer to
//! the original saved vector, including expectations and best-effort commands.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step<'a> {
    Expectation(Precondition<'a>),
    Try { note: &'a str, args: &'a [String] },
    ReviewedPush,
    PreflightFix,
    GerritAmend,
    Git,
}

/// Preconditions remain immutable and are rechecked before preparation/dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Precondition<'a> {
    Ref {
        name: &'a str,
        tip: &'a str,
    },
    Symref {
        name: &'a str,
        target: &'a str,
        directory: Option<&'a str>,
    },
    Tree {
        revision: &'a str,
        tree: &'a str,
    },
    HttpsRoute(&'a str),
    FoldSafe(&'a str),
    FoldIgnored(&'a str),
    /// Feature-owned guards keep their exact envelope and their existing validator.
    FeatureGuard,
}
impl<'a> Precondition<'a> {
    fn decode(argv: &'a [String]) -> Self {
        if let Some((name, tip)) = expected_ref(argv) {
            return Self::Ref { name, tip };
        }
        if let Some((name, target, directory)) = expected_symref(argv) {
            return Self::Symref {
                name,
                target,
                directory,
            };
        }
        if let Some((revision, tree)) = expected_tree(argv) {
            return Self::Tree { revision, tree };
        }
        match argv {
            [mark, proof] if mark == EXPECT_HTTPS_ROUTE => Self::HttpsRoute(proof),
            [mark, target] if mark == EXPECT_FOLD_SAFE => Self::FoldSafe(target),
            [mark, target] if mark == EXPECT_FOLD_IGNORED => Self::FoldIgnored(target),
            _ => Self::FeatureGuard,
        }
    }
}

pub(super) struct PlannedStep<'a> {
    pub index: usize,
    pub argv: &'a [String],
    pub kind: Step<'a>,
}

/// The persisted representation deliberately stays `Vec<Vec<String>>`: old undo
/// journals decode byte-for-byte, and authentication retries retain their proofs.
pub(super) struct Plan<'a> {
    steps: Vec<PlannedStep<'a>>,
}

impl<'a> Plan<'a> {
    pub fn decode(ops: &'a [Vec<String>]) -> Self {
        let steps = ops
            .iter()
            .enumerate()
            .map(|(index, argv)| {
                let kind = match argv.as_slice() {
                    [mark, note, args @ ..] if mark == TRY => Step::Try { note, args },
                    _ if is_expectation(argv) => Step::Expectation(Precondition::decode(argv)),
                    [mark, ..] if mark == crate::push_preflight::APPROVED => Step::ReviewedPush,
                    [mark, _, _] if mark == crate::push_preflight::FIX => Step::PreflightFix,
                    [mark, ..] if mark == crate::gerrit::AMEND => Step::GerritAmend,
                    _ => Step::Git,
                };
                PlannedStep { index, argv, kind }
            })
            .collect();
        Self { steps }
    }

    pub fn preconditions(&self) -> impl Iterator<Item = Precondition<'a>> + '_ {
        self.steps.iter().filter_map(|step| match step.kind {
            Step::Expectation(condition) => Some(condition),
            _ => None,
        })
    }
    pub fn refs(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.preconditions()
            .filter_map(|condition| match condition {
                Precondition::Ref { name, tip } => Some((name, tip)),
                _ => None,
            })
    }
    pub fn symrefs(&self) -> impl Iterator<Item = (&'a str, &'a str, Option<&'a str>)> + '_ {
        self.preconditions()
            .filter_map(|condition| match condition {
                Precondition::Symref {
                    name,
                    target,
                    directory,
                } => Some((name, target, directory)),
                _ => None,
            })
    }
    pub fn trees(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.preconditions()
            .filter_map(|condition| match condition {
                Precondition::Tree { revision, tree } => Some((revision, tree)),
                _ => None,
            })
    }
    pub fn routes(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.preconditions()
            .filter_map(|condition| match condition {
                Precondition::HttpsRoute(proof) => Some(proof),
                _ => None,
            })
    }

    pub fn steps(&self) -> impl Iterator<Item = &PlannedStep<'a>> {
        self.steps.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_undo_decodes_without_changing_retry_positions_or_proofs() -> Result<(), String> {
        let entry: UndoEntry = serde_json::from_str(
            r#"{"label":"old","undo":[],"redo":[["\u0000expect","HEAD","tip"],["\u0000try","optional","fetch"],["push","origin"]],"heads":null}"#,
        ).map_err(|error| format!("{error:?}"))?;
        assert_eq!(entry.id, 0);
        assert_eq!(entry.agent, None);
        let plan = Plan::decode(&entry.redo);
        let steps: Vec<_> = plan.steps().collect();
        assert!(matches!(
            steps[0].kind,
            Step::Expectation(Precondition::Ref {
                name: "HEAD",
                tip: "tip"
            })
        ));
        assert!(matches!(
            steps[1].kind,
            Step::Try {
                note: "optional",
                ..
            }
        ));
        assert_eq!(steps[2].index, 2);
        assert_eq!(steps[2].argv, entry.redo[2]);
        let mut retry = entry.redo.clone();
        retain_unfinished(&mut retry, steps[2].index);
        assert_eq!(retry, vec![entry.redo[0].clone(), entry.redo[2].clone()]);
        Ok(())
    }

    #[test]
    fn malformed_markers_remain_git_errors_instead_of_becoming_successful_noops()
    -> Result<(), String> {
        let ops = ops(&[&[TRY], &[EXPECT, "HEAD"], &["fetch"]]);
        assert!(Plan::decode(&ops).steps().all(|s| s.kind == Step::Git));
        Ok(())
    }
}

#[cfg(test)]
#[path = "model_pure_tests.rs"]
mod pure_tests;
