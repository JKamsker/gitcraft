#!/usr/bin/env python3
"""Run Cargo's precompiled Windows test binaries in complete, disjoint shards.

Cargo builds all targets once in ci/windows-test-build.py. Every shard downloads
that same artifact and uses the original libtest harness without recompiling.
"""

import argparse
from collections import Counter
import json
import re
from pathlib import Path
import subprocess
import sys


GIT_FIRST = [f"git::{letter}" for letter in "abcdefghijklm"]
REPO_A_C = [f"repo::{letter}" for letter in "abc"]
REPO_D_F = [f"repo::{letter}" for letter in "def"]
REPO_G_M = [f"repo::{letter}" for letter in "ghijklm"]
REPO_N_P = [f"repo::{letter}" for letter in "nop"]
REPO_Q_R = [f"repo::{letter}" for letter in "qr"]
REPO_S = ["repo::s"]
REPO_A_S = REPO_A_C + REPO_D_F + REPO_G_M + REPO_N_P + REPO_Q_R + REPO_S

# These tests run in their own libtest process: some race with concurrent Git
# subprocesses, while the palette performance test has a strict wall-clock budget
# that can be exceeded when other libtest threads contend for the Windows runner.
ISOLATED = {
    # This rewrite test can race with concurrent Git-heavy test processes on Windows.
    # Run it separately from the main repo-d-f libtest harness.
    "repo-d-f": [
        "repo::drag_drop::tests::a_commit_dropped_on_another_of_the_branch_is_squashed_into_it",
        # The lens UI test snapshots the index and drives a Windows ACL-preserving
        # transaction; keep concurrent Git-heavy fixtures from invalidating it.
        "repo::file_comparison::tests::actual_lens_boundary_buttons_stage_and_unstage_the_displayed_snapshot",
    ],
    "repo-q-r": [
        # Closing a live lost-commit scan has hung under parallel Windows fixtures.
        "repo::reflog::tests::closing_the_view_stops_its_scan",
        "repo::reflog::tests::a_commit_lost_in_the_terminal_comes_back_as_a_branch",
        # This end-to-end UI/rebase fixture has hung amid concurrent Windows
        # fixtures. Keep its real confirmation and undo checks in a separate process.
        "repo::rehearse::tests::applying_does_it_for_real_and_it_can_be_undone",
    ],
    # This real rebase/undo fixture hung while other Windows Git fixtures ran.
    "repo-s": ["repo::stack::tests::undoing_a_rebase_that_updated_dependent_branches_puts_them_back"],
    # The end-to-end CLI blame launch hung under concurrent Windows repo-rest tests.
    "repo-rest": ["repo::tests::the_command_line_opens_a_file_s_blame_at_a_line"],
    "core": [
        # Native PowerShell ACL snapshot occasionally exhausts its bounded process
        # deadline when run after other standalone fixtures in the same process.
        "standalone::model::tests::binary_whole_side_choices_preserve_bytes_and_stale_inputs_refuse_save",
        "parallel_bisect::tests::finds_the_first_bad_commit_with_any_parallelism",
        "app::palette::tests::thousands_of_branches_open_and_filter_quickly",
    ],
}

# Native Windows ACL fixtures start PowerShell helpers with bounded deadlines.
# Run their whole feature group serially, outside the other core tests, without
# changing production timeouts or omitting tests from the partition.
SERIAL_GROUPS = {"core": ["standalone::"]}

# libtest accepts multiple substring includes and --skip exclusions.
# Catch-all shards ensure new or unusually named tests cannot silently disappear.
SHARDS = {
    "git-a-m": GIT_FIRST,
    "git-rest": ["git::", *[arg for p in GIT_FIRST for arg in ("--skip", p)]],
    "repo-a-c": [*REPO_A_C, "--skip", "git::"],
    "repo-d-f": [
        *REPO_D_F, "--skip", "git::",
        *[arg for test in ISOLATED["repo-d-f"] for arg in ("--skip", test)],
    ],
    "repo-g-m": [*REPO_G_M, "--skip", "git::"],
    "repo-n-p": [*REPO_N_P, "--skip", "git::"],
    "repo-q-r": [
        *REPO_Q_R, "--skip", "git::",
        *[arg for test in ISOLATED["repo-q-r"] for arg in ("--skip", test)],
    ],
    "repo-s": [
        *REPO_S, "--skip", "git::",
        *[arg for test in ISOLATED["repo-s"] for arg in ("--skip", test)],
    ],
    "repo-rest": [
        "repo::",
        "--skip", "git::",
        *[arg for p in REPO_A_S for arg in ("--skip", p)],
        *[arg for test in ISOLATED["repo-rest"] for arg in ("--skip", test)],
    ],
    "core": [
        "--skip", "git::",
        "--skip", "repo::",
        *[arg for test in ISOLATED["core"] for arg in ("--skip", test)],
        *[arg for group in SERIAL_GROUPS["core"] for arg in ("--skip", group)],
    ],
}


def load_binaries():
    root = Path.cwd().resolve()
    manifest = json.loads((root / "ci" / "test-binaries.json").read_text())
    binaries = []
    for entry in manifest["binaries"]:
        path = (root / entry["path"]).resolve()
        cwd = (root / entry["cwd"]).resolve()
        if not path.is_relative_to(root) or not path.is_file():
            raise RuntimeError(f"Missing or unexpected test executable: {path}")
        if not cwd.is_relative_to(root) or not cwd.is_dir():
            raise RuntimeError(f"Missing or unexpected test working directory: {cwd}")
        binaries.append((path, cwd))
    if not binaries:
        raise RuntimeError("No test binaries in the Windows build artifact")
    return binaries


def inventory(binaries, filters):
    tests = Counter()
    for path, cwd in binaries:
        result = subprocess.run(
            [str(path), "--list", *filters],
            cwd=cwd,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        tests.update(
            line.rsplit(": ", 1)[0]
            for line in result.stdout.splitlines()
            if line.endswith((": test", ": benchmark"))
        )
    return tests


def isolated_inventory(binaries, shard):
    tests = Counter()
    for name in ISOLATED.get(shard, []):
        selected = inventory(binaries, [name, "--exact"])
        if selected != Counter({name: 1}):
            raise RuntimeError(f"Isolated test not found exactly once: {name}: {selected}")
        tests.update(selected)
    return tests


def serial_filters(shard, group):
    # Isolated tests inside a serial feature group must not be run twice.
    return [group, *[arg for name in ISOLATED.get(shard, [])
                     if name.startswith(group) for arg in ("--skip", name)]]


def serial_inventory(binaries, shard):
    tests = Counter()
    for group in SERIAL_GROUPS.get(shard, []):
        selected = inventory(binaries, serial_filters(shard, group))
        if not selected:
            raise RuntimeError(f"Serial test group is empty: {group}")
        if tests & selected:
            raise RuntimeError(f"Serial test groups overlap: {group}")
        tests.update(selected)
    return tests


# These declarations or their owning test modules have #[cfg(unix)].
WINDOWS_INACTIVE_RENAMES = {
    'git::bundles::tests::format::partial_clone_export_never_fetches_promised_blobs',
    'git::bundles::tests::verification::old_git_verification_is_fail_closed_for_promisor_or_unreadable_configuration',
    'git::bundles::tests::verification::verification_streams_only_the_validated_header_and_keeps_the_old_git_fallback',
    'git::drafts::tests::index_safety::configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery',
    'git::drafts::tests::index_safety::private_index_preserves_racy_content_checks_across_timestamp_boundaries',
    'git::drafts::tests::path_safety::changed_symlinks_and_conflicted_indexes_fail_without_following_targets_or_touching_index',
    'git::drafts::tests::path_safety::ignored_rule_read_warnings_fail_closed_before_any_version_is_published',
    'git::drafts::tests::path_safety::missing_paths_below_symlinks_fail_closed_and_do_not_publish_deletions',
    'git::drafts::tests::path_safety::typed_remote_reader_quotes_paths_and_never_confuses_links_with_missing_files',
    'git::drafts::tests::retention::retention_expires_owned_refs_with_cas_and_preserves_unknown_versions',
    'git::drafts::tests::retention::tree_roots_survive_gc_restart_and_do_not_change_history_branches_reflog_or_normal_push',
    'git::external_tool::tests::real::diff::a_diff_tool_that_fails_or_has_nothing_to_show_says_so',
    'git::external_tool::tests::real::diff::a_preset_runs_its_program_with_the_files',
    'git::external_tool::tests::real::diff::a_renamed_file_is_shown_against_what_it_was',
    'git::external_tool::tests::real::diff::the_diff_tool_is_shown_each_kind_of_change',
    'git::file_comparison::tests::paths::link_targets_and_binary_versions_are_read_without_following_links',
    'git::file_comparison::tests::paths::raw_non_utf8_paths_stay_distinct_and_literal_folders_do_not_expand_globs',
    'repo::tests::authentication_integration::an_editor_command_with_arguments_is_run_with_the_path_as_one_argument',
}


def verify_renamed_tests(full, tables, inactive=()):
    """Bind documented decompositions to actual libtest discovery, without running tests."""
    for path, expected in tables:
        pairs = re.findall(r"^\| `([^`]+)` \| `([^`]+)` \|$", path.read_text(), re.M)
        if len(pairs) != expected:
            raise RuntimeError(f"Incomplete rename inventory: {path}: {len(pairs)} != {expected}")
        for previous, current in pairs:
            if current in inactive:
                if full[previous] or full[current]:
                    raise RuntimeError(f"Platform-gated migration unexpectedly present: {current}")
                continue
            if previous == current or full[current] != 1 or full[previous]:
                raise RuntimeError(f"Test migration missing, duplicated or stale: {previous} -> {current}")
    print(f"Verified {sum(count for _, count in tables)} documented test migrations", flush=True)


def verify_partition(binaries):
    full = inventory(binaries, [])
    if not full:
        raise RuntimeError("No tests found; refusing an empty partition")
    verify_renamed_tests(full, [(Path("src/git/maintenance-198.md"), 93),
                                (Path("src/repo/maintenance-198.md"), 39)],
                         WINDOWS_INACTIVE_RENAMES if sys.platform == "win32" else ())
    assigned = Counter()
    counts = {}
    for shard, filters in SHARDS.items():
        tests = inventory(binaries, filters)
        isolated = isolated_inventory(binaries, shard)
        serial = serial_inventory(binaries, shard)
        if tests & isolated or tests & serial or isolated & serial:
            raise RuntimeError(f"Parallel, isolated and serial tests overlap: {shard}")
        tests.update(isolated)
        tests.update(serial)
        if not tests:
            raise RuntimeError(f"Shard {shard} has no tests")
        overlap = assigned & tests
        if overlap:
            raise RuntimeError(f"Tests assigned to multiple shards: {dict(overlap)}")
        assigned.update(tests)
        counts[shard] = tests.total()
    if assigned != full:
        raise RuntimeError(
            f"Partition differs from full Windows test suite: "
            f"missing={dict(full - assigned)}, extra={dict(assigned - full)}"
        )
    print(f"Verified complete, disjoint partition: {counts}; total={full.total()}", flush=True)


def run_shard(binaries, shard):
    filters = SHARDS[shard]
    print(f"Running precompiled Windows test shard {shard}", flush=True)
    ran = 0
    for path, cwd in binaries:
        selected = inventory([(path, cwd)], filters)
        if not selected:
            continue
        print(f"{path.name}: {selected.total()} tests", flush=True)
        result = subprocess.run([str(path), "--nocapture", *filters], cwd=cwd)
        if result.returncode:
            return result.returncode
        ran += selected.total()

    for group in SERIAL_GROUPS.get(shard, []):
        for path, cwd in binaries:
            filters = serial_filters(shard, group)
            selected = inventory([(path, cwd)], filters)
            if not selected:
                continue
            print(f"Running serial group: {group}: {selected.total()} tests", flush=True)
            result = subprocess.run(
                [str(path), "--nocapture", *filters, "--test-threads=1"], cwd=cwd
            )
            if result.returncode:
                return result.returncode
            ran += selected.total()

    for name in ISOLATED.get(shard, []):
        for path, cwd in binaries:
            if inventory([(path, cwd)], [name, "--exact"]):
                print(f"Running isolated test: {name}", flush=True)
                result = subprocess.run([str(path), "--nocapture", name, "--exact"], cwd=cwd)
                if result.returncode:
                    return result.returncode
                ran += 1

    if not ran:
        raise RuntimeError(f"Refusing empty shard {shard}")
    print(f"Passed {ran} tests in {shard}", flush=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", nargs="?", choices=SHARDS)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    if not args.check_only and args.shard is None:
        parser.error("choose a shard or use --check-only")

    binaries = load_binaries()
    if args.check_only:
        verify_partition(binaries)
        return 0
    return run_shard(binaries, args.shard)


if __name__ == "__main__":
    sys.exit(main())
