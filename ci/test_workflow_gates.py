"""Regression checks for the deliberately constrained maintainability workflow ABI.

This is not a general YAML parser. Unsupported mapping/needs layouts fail closed
so a workflow syntax change requires an explicit review of these policy checks.
"""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
GATE = "rust-file-size"
SHARED = "./.github/workflows/rust-file-size.yml"
IDENTIFIER = r"[A-Za-z_][A-Za-z0-9_-]*"
COMPLEXITY_CHECK = "python3 ci/complexity_policy.py --revision HEAD"
COMPILER_CHECK = "python3 -m unittest discover -s ci -p 'test_complexity*.py' -v"
CLIPPY_CHECK = "cargo clippy --locked --all-targets -- -D warnings"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def mapping(text, indent):
    """Read plain mapping entries and retain their nested bodies verbatim."""
    lines = text.splitlines()
    entries = []
    for number, line in enumerate(lines):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        depth = len(line) - len(line.lstrip(" "))
        if depth != indent:
            continue
        match = re.fullmatch(r" " * indent + rf"({IDENTIFIER}):(?:[ ]+(.*))?", line)
        require(match is not None, f"unsupported mapping entry: {line!r}")
        key, scalar = match.groups()
        require(key not in [entry[0] for entry in entries], f"duplicate mapping key: {key}")
        entries.append((key, scalar or "", number))
    return {
        key: (scalar, "\n".join(lines[start + 1:entries[index + 1][2] if index + 1 < len(entries) else len(lines)]))
        for index, (key, scalar, start) in enumerate(entries)
    }


def nested(entries, key, indent):
    require(key in entries, f"missing mapping: {key}")
    scalar, body = entries[key]
    require(not scalar, f"unsupported inline mapping: {key}")
    return mapping(body, indent)


def dependencies(fields):
    if "needs" not in fields:
        return set()
    scalar, body = fields["needs"]
    if scalar:
        require(not body.strip(), "mixed scalar and block needs")
        names = scalar[1:-1].split(",") if scalar.startswith("[") and scalar.endswith("]") else [scalar]
        names = [name.strip() for name in names if name.strip()]
    else:
        names = []
        for line in body.splitlines():
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            match = re.fullmatch(rf"      - ({IDENTIFIER})", line)
            require(match is not None, "unsupported block needs")
            names.append(match.group(1))
    require(all(re.fullmatch(IDENTIFIER, name) for name in names), "unsupported needs expression")
    return set(names)


def job_steps(text, job):
    top = mapping(text, 0)
    jobs = nested(top, "jobs", 2)
    require(job in jobs, f"missing required job: {job}")
    fields = mapping(jobs[job][1], 4)
    require(not ({"if", "continue-on-error", "defaults"} & fields.keys()),
            f"required job {job} can skip, soften, or retarget checks")
    require("defaults" not in top, "workflow defaults can retarget required checks")
    for owner, indent, allowed in ((top, 2, {"CARGO_TERM_COLOR"}),
                                    (fields, 6, {"VERSION", "PKG"})):
        if "env" in owner:
            variables = nested(owner, "env", indent)
            require(variables.keys() <= allowed, "ancestor environment can override required checks")
    require("steps" in fields and not fields["steps"][0], f"missing steps for {job}")
    blocks = re.split(r"(?m)^      - ", fields["steps"][1])
    require(not any(line.strip() and not line.lstrip().startswith("#")
                    for line in blocks[0].splitlines()), "unsupported steps layout")
    steps = [mapping("        " + block, 8) for block in blocks[1:]]
    checkouts = [step for step in steps if step.get("uses", ("",))[0] == "actions/checkout@v4"]
    require(bool(checkouts), f"required job {job} must check out the candidate")
    for step in checkouts:
        if "with" in step:
            options = nested(step, "with", 10)
            require(not ({"repository", "ref", "path"} & options.keys()),
                    "checkout can retarget required checks")
    return steps


def verify_required_commands(text, job, commands):
    steps = job_steps(text, job)
    for command in commands:
        matches = [step for step in steps if step.get("run", (None,))[0] == command]
        require(len(matches) == 1, f"required exact command missing or duplicated: {command}")
        step = matches[0]
        require(not ({"if", "continue-on-error", "working-directory", "shell", "env"} & step.keys()),
                f"required command can skip, soften, or retarget checks: {command}")
        require(not step["run"][1].strip(), f"required command has trailing content: {command}")


def verify_shared_gate(text):
    top = mapping(text, 0)
    require(set(nested(top, "on", 2)) == {"workflow_call"}, "shared gate must be reusable without filters")
    jobs = nested(top, "jobs", 2)
    require(bool(jobs), "shared gate has no jobs")
    # These fields can skip the check, soften failures, or inspect another tree.
    forbidden = r"(?:if|continue-on-error|working-directory|repository|ref|path|paths|paths-ignore)"
    require(not re.search(rf"^\s*(?:- )?{forbidden}:", text, re.M), "shared gate can skip, soften, or retarget checks")
    command = r"^\s+(?:- )?run: python3 ci/rust_file_size\.py --revision HEAD\s*$"
    require(re.search(command, text, re.M), "shared gate must check the complete immutable candidate")
    verify_required_commands(text, "check", (COMPLEXITY_CHECK, COMPILER_CHECK))


def verify_callers(text):
    top = mapping(text, 0)
    require(not re.search(r"^\s+(?:paths|paths-ignore):", top["on"][1], re.M), "workflow path filters skip the gate")
    jobs = nested(top, "jobs", 2)
    require(GATE in jobs, "missing reusable size gate")
    fields = {}
    for name, (scalar, body) in jobs.items():
        require(not scalar, f"unsupported inline job: {name}")
        fields[name] = mapping(body, 4)
    gate = fields[GATE]
    require(gate.get("uses", (None,))[0] == SHARED, "size gate must call the shared workflow")
    require("if" not in gate and "continue-on-error" not in gate, "size gate must be unconditional and required")
    edges = {name: dependencies(job) for name, job in fields.items()}
    for name, job in fields.items():
        require("continue-on-error" not in job, f"job {name} softens required failures")
        condition = job.get("if", ("",))[0]
        if re.search(r"\b(?:always|failure|cancelled)\s*\(", condition):
            # This existing status-only job must run after a failed/skipped
            # Windows dependency to preserve the branch-protection result.
            body = jobs[name][1]
            status_only = (name == "windows-test-result" and edges[name] == {"windows-tests"}
                           and not re.search(r"^\s+(?:- )?uses:", body, re.M)
                           and re.search(r'run: \|\n          test "\$TESTS_RESULT" = success\s*$', body))
            require(status_only, f"job {name} bypasses dependency status")

    def reaches_gate(name, visiting):
        require(name in jobs, f"unknown dependency: {name}")
        require(name not in visiting, f"cyclic dependency: {name}")
        if name == GATE:
            return True
        parents = [reaches_gate(parent, visiting | {name}) for parent in edges[name]]
        return any(parents)

    for name in jobs:
        require(reaches_gate(name, set()), f"job {name} bypasses the size gate")


class WorkflowGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        workflows = ROOT / ".github/workflows"
        cls.shared = (workflows / "rust-file-size.yml").read_text(encoding="utf-8")
        cls.callers = {name: (workflows / name).read_text(encoding="utf-8")
                       for name in ("ci.yml", "release.yml")}
        cls.clippy = [(name, (workflows / name).read_text(encoding="utf-8"), job)
                      for name, job in (("ci.yml", "lint"), ("release.yml", "build"),
                                        ("windows-tests.yml", "windows-build"))]

    def test_shared_gate_checks_the_full_immutable_candidate(self):
        verify_shared_gate(self.shared)

    def test_every_ci_and_release_job_depends_on_the_gate(self):
        for name, text in self.callers.items():
            with self.subTest(workflow=name):
                verify_callers(text)

    def test_every_build_workflow_requires_strict_all_target_clippy(self):
        for name, text, job in self.clippy:
            with self.subTest(workflow=name):
                verify_required_commands(text, job, (CLIPPY_CHECK,))

    def required_commands(self):
        yield "shared-policy", self.shared, "check", COMPLEXITY_CHECK
        yield "shared-compiler", self.shared, "check", COMPILER_CHECK
        for name, text, job in self.clippy:
            yield name, text, job, CLIPPY_CHECK

    def test_required_complexity_commands_cannot_be_removed_or_weakened(self):
        weakened = {
            COMPLEXITY_CHECK: (COMPLEXITY_CHECK.replace("--revision HEAD", "--staged"),
                               COMPLEXITY_CHECK.replace("HEAD", "HEAD~1")),
            COMPILER_CHECK: (COMPILER_CHECK.replace("test_complexity*.py", "test_complexity_policy.py"),),
            CLIPPY_CHECK: (CLIPPY_CHECK.replace("--all-targets", "--lib"),
                          CLIPPY_CHECK.replace("--locked ", ""),
                          CLIPPY_CHECK.replace("-D warnings", "-W warnings"),
                          CLIPPY_CHECK + " --cap-lints allow"),
        }
        for name, text, job, command in self.required_commands():
            for replacement in ("", command + " || true", command + " && echo ignored", *weakened[command]):
                with self.subTest(workflow=name, replacement=replacement), self.assertRaisesRegex(ValueError, "exact command"):
                    verify_required_commands(text.replace(command, replacement), job, (command,))

    def test_required_complexity_steps_cannot_skip_soften_or_retarget_checks(self):
        for name, text, job, command in self.required_commands():
            for field in ("if: false", "continue-on-error: true", "working-directory: elsewhere",
                          "shell: echo {0}", "env:\n          RUSTFLAGS: --cap-lints allow"):
                changed = text.replace(f"        run: {command}", f"        {field}\n        run: {command}")
                with self.subTest(workflow=name, field=field), self.assertRaisesRegex(ValueError, "skip, soften, or retarget"):
                    verify_required_commands(changed, job, (command,))

    def test_required_jobs_and_checkouts_cannot_skip_soften_or_retarget_checks(self):
        for name, text, job, command in self.required_commands():
            for field in ("if: false", "continue-on-error: true", "defaults:\n      run:\n        working-directory: elsewhere"):
                changed = text.replace(f"  {job}:\n", f"  {job}:\n    {field}\n")
                with self.subTest(workflow=name, field=field), self.assertRaisesRegex(ValueError, "skip, soften, or retarget"):
                    verify_required_commands(changed, job, (command,))
            for field in ("repository: other/project", "ref: other-branch", "path: elsewhere"):
                changed = text.replace("uses: actions/checkout@v4", f"uses: actions/checkout@v4\n        with:\n          {field}")
                with self.subTest(workflow=name, checkout=field), self.assertRaisesRegex(ValueError, "retarget"):
                    verify_required_commands(changed, job, (command,))

    def test_ancestor_environment_cannot_override_required_compiler_checks(self):
        overrides = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                     "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", "CLIPPY_CONF_DIR",
                     "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "PATH")
        for name, text, job, command in self.required_commands():
            for variable in overrides:
                for scope in ("workflow", "job"):
                    if scope == "workflow":
                        # Existing workflow envs are parsed by key, not substring matches
                        # against a nested job env or an unrelated step's environment.
                        if "env" in mapping(text, 0):
                            changed = text.replace("\nenv:\n", f"\nenv:\n  {variable}: --cap-lints allow\n", 1)
                        else:
                            changed = text + f"\nenv:\n  {variable}: --cap-lints allow\n"
                    else:
                        start = text.index(f"  {job}:\n")
                        prefix, body = text[:start], text[start:]
                        job_fields = mapping(nested(mapping(text, 0), "jobs", 2)[job][1], 4)
                        if "env" in job_fields:
                            body = body.replace("    env:\n", f"    env:\n      {variable}: --cap-lints allow\n", 1)
                        else:
                            body = body.replace(f"  {job}:\n", f"  {job}:\n    env:\n      {variable}: --cap-lints allow\n", 1)
                        changed = prefix + body
                    with self.subTest(workflow=name, scope=scope, variable=variable), self.assertRaisesRegex(ValueError, "ancestor environment"):
                        verify_required_commands(changed, job, (command,))

    def test_shared_gate_rejects_conditional_softened_or_retargeted_checks(self):
        for field in ("if: false", "continue-on-error: true", "working-directory: elsewhere",
                      "repository: other/project", "ref: other-branch", "path: elsewhere"):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "skip, soften, or retarget"):
                verify_shared_gate(self.shared + "\n        " + field + "\n")

    def test_shared_gate_rejects_partial_or_ignored_checker_commands(self):
        for replacement in ("--staged", "--revision HEAD || true", "--revision HEAD~1", ""):
            with self.subTest(replacement=replacement), self.assertRaisesRegex(ValueError, "immutable candidate"):
                verify_shared_gate(self.shared.replace("--revision HEAD", replacement))

    def test_callers_reject_skipped_or_optional_gate_jobs(self):
        for field in ("if: false", "continue-on-error: true"):
            for name, text in self.callers.items():
                with self.subTest(workflow=name, field=field), self.assertRaisesRegex(ValueError, "unconditional"):
                    verify_callers(text.replace(f"  {GATE}:\n", f"  {GATE}:\n    {field}\n"))

    def test_callers_reject_path_filters_and_alternate_workflows(self):
        for name, text in self.callers.items():
            with self.subTest(workflow=name):
                with self.assertRaisesRegex(ValueError, "path filters"):
                    verify_callers(text.replace("  push:\n", "  push:\n    paths: ['src/**']\n"))
                with self.assertRaisesRegex(ValueError, "shared workflow"):
                    verify_callers(text.replace(SHARED, "./.github/workflows/optional.yml"))

    def test_new_jobs_and_removed_dependencies_cannot_bypass_the_gate(self):
        for name, text in self.callers.items():
            with self.subTest(workflow=name):
                with self.assertRaisesRegex(ValueError, "bypasses"):
                    verify_callers(text + "\n  ungated:\n    runs-on: ubuntu-latest\n")
                with self.assertRaisesRegex(ValueError, "bypasses"):
                    verify_callers(text.replace("    needs: rust-file-size\n", ""))

    def test_dependency_status_and_failure_softening_cannot_bypass_the_gate(self):
        for name, text in self.callers.items():
            with self.subTest(workflow=name):
                for field in ("if: always()", "continue-on-error: true"):
                    with self.subTest(field=field), self.assertRaisesRegex(ValueError, "bypasses dependency status|softens"):
                        verify_callers(text.replace("    needs: rust-file-size\n", f"    needs: rust-file-size\n    {field}\n", 1))
        with self.assertRaisesRegex(ValueError, "bypasses dependency status"):
            verify_callers(self.callers["ci.yml"].replace('test "$TESTS_RESULT" = success', 'echo ignored'))

    def test_unknown_and_cyclic_dependencies_fail_even_with_a_gate_dependency(self):
        for dependency in ("missing", "ungated"):
            text = self.callers["ci.yml"] + f"\n  ungated:\n    needs: [rust-file-size, {dependency}]\n"
            with self.subTest(dependency=dependency), self.assertRaisesRegex(ValueError, "unknown|cyclic"):
                verify_callers(text)

    def test_mapping_and_dependency_syntax_fail_closed(self):
        for text in ("jobs: {}", "jobs: &shared", "jobs:\njobs:"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                nested(mapping(text, 0), "jobs", 2)
        for scalar in ("${{ needs.dynamic }}", "[gate, ${{ needs.dynamic }}]"):
            with self.subTest(scalar=scalar), self.assertRaisesRegex(ValueError, "unsupported needs"):
                dependencies({"needs": (scalar, "")})


if __name__ == "__main__":
    unittest.main()
