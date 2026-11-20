//! Lossless decoding boundary for persisted operation argv. Indices always refer to
//! the original saved vector, including expectations and best-effort commands.
use super::*;

#[path = "model_decode.rs"]
mod decode;

/// What one saved step is. Every internal marker decodes to its own variant with its parsed
/// operands; only a step that starts with a Git word is `Git`. The view is read-only: it never
/// rewrites the saved argv, and `PlannedStep::argv` keeps the exact original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step<'a> {
    Expectation(Precondition<'a>),
    /// `[TRY, note, args..]`: runs `args` unattended; a failure only adds `note`.
    Try {
        note: &'a str,
        args: &'a [String],
    },
    /// `[APPROVED, id]` or `[APPROVED, id, "-c", "gitcraft.commandSource=…"]`: the push
    /// preflight registered as `id`, with the command-source evidence it bound, if any.
    ReviewedPush {
        id: &'a str,
        evidence: Option<&'a [String]>,
    },
    /// `[FIX, id, fix]`: the fix `fix` of the push preflight registered as `id`.
    PreflightFix {
        id: &'a str,
        fix: &'a str,
    },
    /// `[AMEND, args..]`: the `commit --amend` argv that `gerrit::amend_args` validates and
    /// prepares at dispatch (it owns what a well-formed one is, so any length decodes).
    GerritAmend {
        args: &'a [String],
    },
    /// `[APPLY, json]`: the tracking change `tracking::execute` applies instead of Git.
    TrackingApply {
        json: &'a str,
    },
    Git,
    /// An internal marker its producers never write this way. Execution treats it exactly
    /// like `Git`, as it did before this variant existed: nothing internal runs, and Git
    /// refuses an argument that holds a NUL.
    Malformed(Malformed<'a>),
}

/// Why a step that starts with an internal marker has no meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Malformed<'a> {
    /// A known marker followed by `count` operands in a shape its producer never writes.
    Operands { marker: &'a str, count: usize },
    /// A NUL-prefixed first word that is no step marker (a graph stand-in hash, or new).
    UnknownMarker(&'a str),
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
    /// `[gerrit::CHECK, base, head]`: the reviewed stack `base..head` is still presentable.
    GerritStack {
        base: &'a str,
        head: &'a str,
    },
    /// `[forks::guard::MARK, json]`: the remote a fork action planned against, as the
    /// `Destination` JSON that `forks::guard::check` parses and compares.
    ForkDestination(&'a str),
    /// `[forks::guard::CHECKOUT_MARK, json]`: the checkout destination JSON that
    /// `CheckoutReplay::read` parses and validates.
    ForkCheckout(&'a str),
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
            .map(|(index, argv)| PlannedStep {
                index,
                argv,
                kind: decode::step(argv),
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
    pub fn gerrit_stacks(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.preconditions()
            .filter_map(|condition| match condition {
                Precondition::GerritStack { base, head } => Some((base, head)),
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
    fn malformed_markers_are_explicit_instead_of_becoming_successful_noops() {
        let ops = ops(&[&[TRY], &[EXPECT, "HEAD"], &["fetch"]]);
        let kinds: Vec<_> = Plan::decode(&ops).steps().map(|s| s.kind).collect();
        let arity = |marker, count| Step::Malformed(Malformed::Operands { marker, count });
        assert_eq!(kinds, [arity(TRY, 0), arity(EXPECT, 1), Step::Git]);
    }
}

#[cfg(test)]
#[path = "model_pure_tests.rs"]
mod pure_tests;

#[cfg(test)]
#[path = "model_decode_tests.rs"]
mod decode_tests;

#[cfg(test)]
#[path = "model_parity_tests.rs"]
mod parity_tests;
