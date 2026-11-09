import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('integrity', Path(__file__).with_name('gitcraft_pr184_source_integrity.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class SourceIntegrity(unittest.TestCase):
    def test_hidden_edits_missing_modes_and_symlinks(self):
        with tempfile.TemporaryDirectory(prefix='gitcraft-source-integrity-fixture-') as root:
            root = Path(root)
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=root)
            git('init', '-q')
            (root/'Cargo.toml').write_text('original manifest\n')
            (root/'source.rs').write_text('original source\n')
            (root/'link').symlink_to('source.rs')
            git('add', '.')
            git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture')
            commit = git('rev-parse','HEAD').decode().strip()
            def inspect():
                return module.inspect_source(root, commit)
            initial = inspect()
            self.assertEqual(initial['errors'], [])
            self.assertEqual(initial['expected_map_sha256'], initial['observed_map_sha256'])
            git('update-index','--assume-unchanged','source.rs')
            (root/'source.rs').write_text('hidden changed source\n')
            self.assertEqual(git('status','--porcelain'), b'')
            self.assertTrue(any('bytes/mode differ' in e for e in inspect()['errors']))
            git('update-index','--no-assume-unchanged','source.rs')
            git('checkout','--','source.rs')
            (root/'Cargo.toml').unlink()
            self.assertTrue(inspect()['errors'])
            git('checkout','--','Cargo.toml')
            (root/'source.rs').chmod(0o755)
            self.assertTrue(inspect()['errors'])
            (root/'source.rs').chmod(0o644)
            (root/'link').unlink()
            (root/'link').symlink_to('Cargo.toml')
            self.assertTrue(inspect()['errors'])
            git('checkout','--','link')
            git('update-index','--skip-worktree','source.rs')
            self.assertTrue(any('sparse index flag' in e for e in inspect()['errors']))

if __name__ == '__main__':
    unittest.main()
