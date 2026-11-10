use super::*;

pub(super) type TemplateSpec = (&'static str, &'static str, &'static [(&'static str, bool)]);

/// Localize finite application-owned operation labels at presentation only.
/// Raw labels remain the identity used by operations, undo, versions and LFS.
/// Unknown provider/custom labels are opaque and are returned unchanged.
pub(crate) fn display_operation(label: &str) -> String {
    operation_display(label, 0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PaintFamily {
    CoreLines,
    LegacyPrefix,
    WorkflowFixed,
    Review,
    Workflow,
    Legacy,
}

pub(super) enum Painted {
    Known(String),
    Opaque,
}
impl Painted {
    fn text(self, raw: &str) -> String {
        match self {
            Self::Known(text) => text,
            Self::Opaque => raw.to_owned(),
        }
    }
}
pub(super) fn known_operation(label: &str, depth: usize) -> Option<Painted> {
    review::fixed(label)
        .map(Painted::Known)
        .or_else(|| workflow::shared_fixed(label).map(Painted::Known))
        .or_else(|| operation_template(label, depth, PaintFamily::WorkflowFixed))
        .or_else(|| operation_display_inner(label, depth, PaintFamily::LegacyPrefix))
        .or_else(|| operation_template(label, depth, PaintFamily::CoreLines))
        .or_else(|| operation_display_inner(label, depth, PaintFamily::Review))
        .or_else(|| operation_display_inner(label, depth, PaintFamily::Workflow))
        .or_else(|| operation_display_inner(label, depth, PaintFamily::Legacy))
}

pub(super) fn operation_display(label: &str, depth: usize) -> String {
    known_operation(label, depth).map_or_else(|| label.to_owned(), |painted| painted.text(label))
}

pub(super) fn operation_display_inner(
    label: &str,
    depth: usize,
    family: PaintFamily,
) -> Option<Painted> {
    if family == PaintFamily::Review {
        return review::fixed(label)
            .map(Painted::Known)
            .or_else(|| operation_template(label, depth, family));
    }
    if family == PaintFamily::Workflow {
        return workflow::special(label)
            .map(Painted::Known)
            .or_else(|| operation_template(label, depth, family));
    }
    if matches!(family, PaintFamily::Legacy | PaintFamily::LegacyPrefix) {
        if family == PaintFamily::Legacy {
            let shared = crate::operation_labels::display(label);
            if shared != label {
                return Some(Painted::Known(shared));
            }
        }
        if depth < 4 {
            for (prefix, id) in [
                ("Undo ", "interaction-operation-undo"),
                ("Redo ", "interaction-operation-redo"),
                ("Preparing ", "interaction-operation-preparing"),
                ("Push after ", "interaction-operation-push-after"),
            ] {
                if let Some(inner) = label.strip_prefix(prefix) {
                    let rendered = operation_display(inner, depth + 1);
                    if rendered != inner {
                        return Some(Painted::Known(crate::i18n::message(
                            id,
                            &[("operation", rendered.into())],
                        )));
                    }
                    // A custom inner command must stay opaque as an entire wrapper. Do not
                    // let a more general label/path template reinterpret its first word.
                    return Some(Painted::Opaque);
                }
            }
        }
        for (suffix, id) in [
            (" (here only)", "repo-core-operation-dco-here"),
            ("", "repo-core-operation-dco"),
        ] {
            if let Some(branch) = label
                .strip_prefix("Sign off the commits of ")
                .and_then(|v| v.strip_suffix(suffix))
                && !branch.is_empty()
            {
                return Some(Painted::Known(crate::i18n::message(
                    id,
                    &[("branch", branch.into())],
                )));
            }
        }
    }
    if family == PaintFamily::LegacyPrefix {
        return None;
    }
    operation_template(label, depth, family)
}

pub(crate) fn display_status(status: &str) -> String {
    if let Some(rest) = status.strip_prefix(crate::git::CANCELLED) {
        const PULL_NOTE: &str = "\nIf the pull had started merging or rebasing, that is left in progress: Abort in the toolbar goes back to where the branch was.";
        let (body, pull) = rest
            .strip_suffix(PULL_NOTE)
            .map_or((rest, false), |body| (body, true));
        let rendered = if body.is_empty() {
            Some(crate::i18n::text("repo-core-cancelled").to_owned())
        } else {
            [(
                "\nUntil it is deleted, git refuses to work in this repository. Delete it if no other git program is running here.", "one"
            ), (
                "\nUntil them are deleted, git refuses to work in this repository. Delete them if no other git program is running here.", "many"
            )].into_iter().find_map(|(suffix, amount)| {
                body.strip_prefix(", but git left ").and_then(|v| v.strip_suffix(suffix)).and_then(|v| v.strip_suffix(" behind"))
                    .map(|locks| crate::i18n::message("repo-core-cancelled-locks", &[("locks", locks.into()), ("amount", amount.into())]))
            })
        };
        if let Some(mut rendered) = rendered {
            if pull {
                rendered.push_str(crate::i18n::text("repo-core-4160"));
            }
            return rendered;
        }
    }
    if let Some(rendered) = known_operation(status, 0) {
        return rendered.text(status);
    }
    if let Some((label, detail)) = status.split_once(": ")
        && let Some(Painted::Known(label)) = known_operation(label, 0)
    {
        return format!(
            "{}: {}",
            label,
            crate::git::display_error(&crate::forge::inbox::display_error(detail))
        );
    }
    crate::git::display_error(&crate::forge::inbox::display_error(status))
}

/// Paint source-owned Rust dependency reports without interpreting arbitrary operations.
pub(crate) fn display_review_dependency(raw: &str) -> String {
    review::dependency(raw)
}
/// Paint the intent collector's quarantine envelope; refs and unknown details stay opaque.
pub(crate) fn display_intent_warning(raw: &str) -> String {
    review::intent_warning(raw)
}

pub(super) fn review_owned(raw: &str) -> String {
    operation_display_inner(raw, 0, PaintFamily::Review)
        .map_or_else(|| raw.to_owned(), |painted| painted.text(raw))
}

pub(super) fn workflow_argument(id: &str, name: &str, raw: &str, depth: usize) -> Option<String> {
    use workflow::Adapter;
    match workflow::adapter(id, name) {
        Some(Adapter::Operation) if depth < 4 => {
            known_operation(raw, depth + 1).and_then(|painted| match painted {
                Painted::Known(text) => Some(text),
                Painted::Opaque => None,
            })
        }
        Some(Adapter::Operation) => None,
        Some(Adapter::Scope) => workflow::scope(raw),
        Some(Adapter::Health) => workflow::health(raw),
        Some(Adapter::Pick) if matches!(raw, "Stage" | "Unstage" | "Discard" | "Revert") => {
            known_operation(raw, depth + 1).and_then(|painted| match painted {
                Painted::Known(text) => Some(text),
                Painted::Opaque => None,
            })
        }
        Some(Adapter::Pick) => None,
        Some(Adapter::Noun) => workflow::noun(raw),
        Some(Adapter::Headline) => workflow::headline(raw),
        Some(Adapter::Place) => workflow::place(raw),
        Some(Adapter::InProgress) => workflow::in_progress(raw),
        Some(Adapter::Receipt) => workflow::receipt(raw),
        None => Some(raw.to_owned()),
    }
}
