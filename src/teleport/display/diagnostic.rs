//! Diagnostic responsibilities.
use super::*;

/// Known application diagnostics only; raw engine/plan/cancellation values stay unchanged.
pub(crate) fn diagnostic(raw: &str) -> String {
    diagnostic_at(raw, 0)
}

pub(super) fn diagnostic_at(raw: &str, depth: usize) -> String {
    if depth >= 8 {
        return raw.to_owned();
    }
    let id = match raw {
        "It stopped unexpectedly." => "interaction-teleport-error-stopped",
        "it stopped unexpectedly." => "interaction-teleport-error-stopped-lower",
        "Pick a machine to teleport to." => "interaction-teleport-error-pick-machine",
        "Pick the repository the work goes in." => "interaction-teleport-error-pick-repo",
        "Machine discovery stopped; use the tab's Teleport menu to type a host." => {
            "interaction-teleport-error-discovery"
        }
        "The host is what ssh takes: an alias of ~/.ssh/config or user@host." => {
            "interaction-teleport-error-host"
        }
        "could not find the home folder" => "interaction-teleport-error-home",
        "Teleport cancelled: nothing was changed." => "interaction-teleport-error-cancelled",
        "The new worktree has to be on the machine of its repository." => {
            "interaction-teleport-error-worktree-machine"
        }
        "The bundle couldn't be written." => "interaction-teleport-error-write-bundle",
        "Nothing is committed here yet: commit once, then teleport." => {
            "interaction-teleport-error-no-commit"
        }
        "Some files have conflicts: resolve them first." => "interaction-teleport-error-conflicts",
        "This is a shallow clone: its history stops short, and the repository there would be missing the commits it builds on. Fetch the whole history first (git fetch --unshallow)." => {
            "interaction-teleport-error-shallow"
        }
        "This repository has submodules, which teleport doesn't carry." => {
            "interaction-teleport-error-submodules"
        }
        "The checked-out commit changed while the teleport got ready: try again." => {
            "interaction-teleport-error-head-changed"
        }
        "the copy is on another machine" => "interaction-teleport-error-copy-machine",
        "Files kept in Git LFS arrived as their pointers; the palette's LFS: Pull fetches those that were pushed." => {
            "interaction-teleport-error-lfs"
        }
        "Type a folder for the repository there." => {
            "interaction-teleport-error-folder-the-repository-there"
        }
        "Give the repository there as a full path on this machine." => {
            "interaction-teleport-error-local-the-repository-there"
        }
        "Type a folder for the clone." => "interaction-teleport-error-folder-the-clone",
        "Give the clone as a full path on this machine." => {
            "interaction-teleport-error-local-the-clone"
        }
        "Type a folder for the new repository." => {
            "interaction-teleport-error-folder-the-new-repository"
        }
        "Give the new repository as a full path on this machine." => {
            "interaction-teleport-error-local-the-new-repository"
        }
        "Type a folder for the new worktree." => {
            "interaction-teleport-error-folder-the-new-worktree"
        }
        "Give the new worktree as a full path on this machine." => {
            "interaction-teleport-error-local-the-new-worktree"
        }
        _ => "",
    };
    if !id.is_empty() {
        return text(id).to_owned();
    }
    if let Some(detail) = raw.strip_prefix("Teleport failed: ") {
        return message(
            "interaction-teleport-error-failed",
            &[("detail", (diagnostic_at(detail, depth + 1)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Couldn't take a snapshot of the working tree: ") {
        return message(
            "interaction-teleport-error-snapshot",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) =
        raw.strip_prefix("Some of the snapshot stayed on this repository's machine: ")
    {
        return message(
            "interaction-teleport-error-snapshot-left",
            &[("detail", (diagnostic_at(detail, depth + 1)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not copy ") {
        return message(
            "interaction-teleport-error-copy",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not remove ") {
        return message(
            "interaction-teleport-error-remove",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not look at ") {
        return message(
            "interaction-teleport-error-look",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not make ") {
        return message(
            "interaction-teleport-error-make",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not write ") {
        return message(
            "interaction-teleport-error-write",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not open ") {
        return message(
            "interaction-teleport-error-open",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not read ") {
        return message(
            "interaction-teleport-error-read",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Could not ask ") {
        return message(
            "interaction-teleport-error-ask",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Could not clone ") {
        return message(
            "interaction-teleport-error-clone",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("The bundle couldn't be unpacked on ") {
        return message(
            "interaction-teleport-error-unpack",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("The bundle didn't check out on ") {
        return message(
            "interaction-teleport-error-verify",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("The bundle stayed on ") {
        return message(
            "interaction-teleport-error-bundle-left",
            &[("detail", (detail.to_owned()).into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("could not find the home folder on ") {
        return message(
            "interaction-teleport-error-home-there",
            &[("detail", (native_detail(detail, depth)).into())],
        );
    }
    if let Some(path)=raw.strip_suffix(" is marked so that git doesn't look at its changes (assume-unchanged or skip-worktree), and they wouldn't be carried. Clear the mark first."){return message("interaction-teleport-error-hidden-changes", &[("path",path.into())]);}
    if raw == "A merge is in progress here: finish or abort it first." {
        return text("interaction-teleport-error-operation-merge").to_owned();
    }
    if raw == "A rebase is in progress here: finish or abort it first." {
        return text("interaction-teleport-error-operation-rebase").to_owned();
    }
    if raw == "A cherry-pick is in progress here: finish or abort it first." {
        return text("interaction-teleport-error-operation-cherry-pick").to_owned();
    }
    if raw == "A revert is in progress here: finish or abort it first." {
        return text("interaction-teleport-error-operation-revert").to_owned();
    }
    if raw == "A patch is in progress here: finish or abort it first." {
        return text("interaction-teleport-error-operation-patch").to_owned();
    }
    if let Some(machine) = raw
        .strip_prefix("Give the repository there as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x or ~/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-repository-there-ssh",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the repository there as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-repository-there-wsl",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the clone as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x or ~/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-clone-ssh",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the clone as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-clone-wsl",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the new repository as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x or ~/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-new-repository-ssh",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the new repository as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-new-repository-wsl",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the new worktree as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x or ~/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-new-worktree-ssh",
            &[("machine", machine.into())],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("Give the new worktree as a full path on ")
        .and_then(|s| s.strip_suffix(": /home/me/x."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-the-new-worktree-wsl",
            &[("machine", machine.into())],
        );
    }
    if let Some((value, machine)) = raw
        .strip_suffix(": pick another folder.")
        .and_then(|s| s.rsplit_once(" is already there on "))
        && !value.is_empty()
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-existing-folder",
            &[
                ("path", value.into()),
                ("machine", display_machine(machine).into()),
            ],
        );
    }
    if let Some((value, machine)) = raw
        .strip_suffix(" meanwhile: pick another folder.")
        .and_then(|s| s.rsplit_once(" appeared on "))
        && !value.is_empty()
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-appeared-folder",
            &[
                ("path", value.into()),
                ("machine", display_machine(machine).into()),
            ],
        );
    }
    if let Some((value, machine)) = raw
        .strip_suffix(" meanwhile: pick another branch name.")
        .and_then(|s| s.rsplit_once(" appeared on "))
        && !value.is_empty()
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-appeared-branch",
            &[
                ("name", value.into()),
                ("machine", display_machine(machine).into()),
            ],
        );
    }
    if let Some((value, machine)) = raw
        .strip_suffix(" has commits that aren't here: pick another branch name.")
        .and_then(|s| s.rsplit_once(" on "))
        && !value.is_empty()
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-diverged",
            &[
                ("name", value.into()),
                ("machine", display_machine(machine).into()),
            ],
        );
    }
    if let Some(name) = raw.strip_suffix(" isn't a name git takes for a branch.")
        && !name.is_empty()
    {
        return message(
            "interaction-teleport-error-invalid-branch",
            &[("name", name.into())],
        );
    }
    if let Some((name, rest)) = raw
        .strip_suffix(": pick another branch name.")
        .and_then(|s| s.split_once(" is checked out on "))
        && let Some((machine, path)) = rest.split_once(" already, in ")
        && !name.is_empty()
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-checked-out",
            &[
                ("name", name.into()),
                ("machine", display_machine(machine).into()),
                ("path", path.into()),
            ],
        );
    }
    if let Some(machine) = raw
        .strip_prefix("The bundle that arrived on ")
        .and_then(|s| s.strip_suffix(" didn't hold the work."))
        && valid_machine_label(machine)
    {
        return message(
            "interaction-teleport-error-missing-work",
            &[("machine", display_machine(machine).into())],
        );
    }
    if let Some((machine, rest)) = raw
        .strip_prefix("The repository on ")
        .and_then(|s| s.split_once(" uses "))
        && let Some((theirs, ours)) = rest
            .strip_suffix(": their commits can't be mixed.")
            .and_then(|s| s.split_once(" hashes and this one "))
        && valid_machine_label(machine)
        && ["sha1", "sha256"].contains(&theirs)
        && ["sha1", "sha256"].contains(&ours)
    {
        return message(
            "interaction-teleport-error-hash-format",
            &[
                ("machine", display_machine(machine).into()),
                ("theirs", theirs.into()),
                ("ours", ours.into()),
            ],
        );
    }
    if let Some((sizes, machine)) = raw
        .strip_prefix("Only ")
        .and_then(|s| s.strip_suffix('.'))
        .and_then(|s| s.rsplit_once(" arrived on "))
        && let Some((arrived, total)) = sizes.split_once(" of the bundle's ")
        && valid_machine_label(machine)
        && let (Some(arrived), Some(total)) = (stored_size(arrived), stored_size(total))
    {
        return message(
            "interaction-teleport-error-partial",
            &[
                ("machine", display_machine(machine).into()),
                ("arrived", arrived.into()),
                ("total", total.into()),
            ],
        );
    }
    if let Some(code) = raw
        .strip_prefix("exit status ")
        .and_then(|s| s.parse::<i32>().ok())
    {
        return message("interaction-teleport-error-exit", &[("code", code.into())]);
    }
    crate::git::display_error(raw)
}

// Only the two literal fallback reasons generated by files.rs are interpreted. Other
// native diagnostics (including embedded colons/newlines) remain opaque data.
pub(super) fn native_detail(raw: &str, depth: usize) -> String {
    let Some((where_, reason)) = raw.rsplit_once(": ") else {
        return raw.to_owned();
    };
    let literal_fallback = reason == "the copy is on another machine"
        || reason
            .strip_prefix("exit status ")
            .is_some_and(|n| n.parse::<i32>().is_ok());
    if literal_fallback {
        format!("{where_}: {}", diagnostic_at(reason, depth + 1))
    } else {
        raw.to_owned()
    }
}

pub(super) fn valid_machine_label(raw: &str) -> bool {
    raw == "this machine"
        || raw
            .strip_prefix("SSH · ")
            .is_some_and(crate::remote::valid_ssh_host)
        || raw
            .strip_prefix("WSL · ")
            .is_some_and(crate::remote::valid_distro)
}

pub(super) fn display_machine(raw: &str) -> String {
    if raw == "this machine" {
        super::super::Machine::Here.display_label()
    } else {
        raw.to_owned()
    }
}

pub(super) fn stored_size(raw: &str) -> Option<String> {
    let (number, unit) = raw.split_once(' ')?;
    if !["B", "KB", "MB", "GB", "TB"].contains(&unit)
        || !number.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return None;
    }
    let value = number.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    if unit == "B" && !number.contains('.') {
        Some(message(
            "interaction-teleport-bytes",
            &[("value", value.into())],
        ))
    } else {
        Some(message(
            "interaction-teleport-scaled",
            &[("value", value.into()), ("unit", unit.into())],
        ))
    }
}
