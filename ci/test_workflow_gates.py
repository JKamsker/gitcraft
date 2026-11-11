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

    def test_shared_gate_checks_the_full_immutable_candidate(self):
        verify_shared_gate(self.shared)

    def test_every_ci_and_release_job_depends_on_the_gate(self):
        for name, text in self.callers.items():
            with self.subTest(workflow=name):
                verify_callers(text)

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
