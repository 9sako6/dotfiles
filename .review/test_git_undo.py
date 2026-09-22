"""Verify undo through real Git state, including unborn HEAD and subdirectories."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]


class GitUndoTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.environment = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1'}
        self.git('init', '-q', '-b', 'master')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        self.git('config', 'core.hooksPath', os.devnull)
        for name in ['work/a', 'sibling/b']:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('original')

    def git(self, *args, check=True):
        return subprocess.run(['git', '-C', str(self.root), *args], env=self.environment,
                              capture_output=True, text=True, check=check)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')

    def undo(self, *args, directory='.'):
        return subprocess.run(['/bin/sh', str(REPOSITORY / 'home/mybin/git-undo'), *args],
                              cwd=self.root / directory, env=self.environment,
                              capture_output=True, text=True, timeout=10)

    def staged(self):
        return self.git('diff', '--cached', '--name-only').stdout.splitlines()

    def test_no_arguments_unstages_the_whole_repository_from_a_subdirectory(self):
        self.commit()
        head = self.git('rev-parse', 'HEAD').stdout
        (self.root / 'sibling/b').write_text('changed')
        self.git('add', '.')
        result = self.undo(directory='work')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.staged(), [])
        self.assertEqual(self.git('rev-parse', 'HEAD').stdout, head)
        self.assertEqual((self.root / 'sibling/b').read_text(), 'changed')

    def test_initial_commit_can_be_undone_then_unstaged_without_losing_files(self):
        self.commit()
        self.assertEqual(self.undo().returncode, 0)
        self.assertNotEqual(self.git('rev-parse', '--verify', 'HEAD', check=False).returncode, 0)
        self.assertEqual(self.staged(), ['sibling/b', 'work/a'])
        result = self.undo(directory='work')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.staged(), [])
        for name in ['work/a', 'sibling/b']:
            self.assertEqual((self.root / name).read_text(), 'original')

    def test_explicit_paths_remain_relative_and_leave_other_entries_staged(self):
        self.commit()
        for name in ['work/a', 'sibling/b']:
            (self.root / name).write_text('changed')
        self.git('add', '.')
        result = self.undo('a', directory='work')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.staged(), ['sibling/b'])
        self.assertEqual((self.root / 'work/a').read_text(), 'changed')

    def test_unborn_explicit_path_preserves_unstaged_edits_and_other_index_entries(self):
        self.git('add', '.')
        (self.root / 'work/a').write_text('edited after staging')
        result = self.undo('a', directory='work')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.staged(), ['sibling/b'])
        self.assertEqual((self.root / 'work/a').read_text(), 'edited after staging')

    def test_missing_staged_path_does_not_undo_a_commit(self):
        self.commit()
        head = self.git('rev-parse', 'HEAD').stdout
        result = self.undo('a', directory='work')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.git('rev-parse', 'HEAD').stdout, head)


if __name__ == '__main__':
    unittest.main()
