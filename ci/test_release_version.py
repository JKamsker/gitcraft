"""Release version transformation fixtures, including the historical RPM failure (issue #211)."""

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import release_version  # noqa: E402

# dev Release 42194556419 failed Linux RPM validation with: invalid version "0.3.4-dev.6".
HISTORICAL_DEV_VERSION = "0.3.4-dev.6"


class RpmVersionTests(unittest.TestCase):
    def test_the_historical_prerelease_is_converted_to_a_valid_rpm_version(self):
        self.assertEqual(release_version.rpm_version(HISTORICAL_DEV_VERSION), "0.3.4~dev.6")

    def test_the_raw_prerelease_is_rejected_like_rpmbuild_does(self):
        with self.assertRaisesRegex(ValueError, 'invalid RPM version "0.3.4-dev.6"'):
            release_version.validate_rpm(HISTORICAL_DEV_VERSION)

    def test_stable_versions_pass_unchanged_and_garbage_fails(self):
        self.assertEqual(release_version.rpm_version("1.2.3"), "1.2.3")
        for bad in ("", "1.2", "1.2.3-", "1.2.3 dev", "v1.2.3"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                release_version.rpm_version(bad)

    def test_the_command_line_prints_the_rpm_version(self):
        self.assertEqual(release_version.main(["rpm", HISTORICAL_DEV_VERSION]), 0)
        self.assertEqual(release_version.main(["rpm", "not-a-version"]), 1)


class ComputeTests(unittest.TestCase):
    def test_a_dev_build_after_a_release_is_the_next_patch_prerelease(self):
        result = release_version.compute("0.3.3", True, "dev", "6", released=lambda version: True)
        self.assertEqual(result, {"version": HISTORICAL_DEV_VERSION, "label": "v0.3.4-dev.6", "prerelease": "true"})

    def test_a_dev_build_before_the_first_tag_keeps_the_cargo_version(self):
        result = release_version.compute("0.3.4", True, "dev", "7", released=lambda version: False)
        self.assertEqual(result["version"], "0.3.4-dev.7")

    def test_a_tag_or_manual_run_builds_cargo_s_version_with_a_safe_label(self):
        result = release_version.compute("0.3.4", False, "feature/x y", "1", released=None)
        self.assertEqual(result, {"version": "0.3.4", "label": "feature-x-y", "prerelease": "false"})


class ApplyTests(unittest.TestCase):
    def test_lf_and_crlf_manifests_and_locks_carry_the_version(self):
        for newline in ("\n", "\r\n"):
            with self.subTest(newline=repr(newline)), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                (root / "Cargo.toml").write_bytes(newline.join(
                    ['[package]', 'name = "gitcraft"', 'version = "0.3.3"', '', '[dependencies]',
                     'x = { version = "1" }', '']).encode())
                (root / "Cargo.lock").write_bytes(newline.join(
                    ['[[package]]', 'name = "egui"', 'version = "0.3.3"', '', '[[package]]',
                     'name = "gitcraft"', 'version = "0.3.3"', '']).encode())
                release_version.apply(root, HISTORICAL_DEV_VERSION)
                manifest = (root / "Cargo.toml").read_bytes().decode()
                lock = (root / "Cargo.lock").read_bytes().decode()
                self.assertIn(f'version = "{HISTORICAL_DEV_VERSION}"', manifest)
                self.assertIn('x = { version = "1" }', manifest)
                self.assertIn(f'name = "gitcraft"{newline}version = "{HISTORICAL_DEV_VERSION}"', lock)
                self.assertIn(f'name = "egui"{newline}version = "0.3.3"', lock)

    def test_the_real_manifest_and_lock_accept_a_prerelease(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temp:
            for name in ("Cargo.toml", "Cargo.lock"):
                (Path(temp) / name).write_bytes((root / name).read_bytes())
            release_version.apply(temp, HISTORICAL_DEV_VERSION)
            self.assertEqual(release_version.cargo_version(temp), HISTORICAL_DEV_VERSION)


if __name__ == "__main__":
    unittest.main()
