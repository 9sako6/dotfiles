"""NUL-safe filenames and fail-closed inspection through the public checker."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]


class PrivacyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.environment = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1'}
        self.git('init', '-q')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'core.hooksPath', os.devnull)
        self.git('config', 'commit.gpgsign', 'false')

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.root), *args], env=self.environment,
                              capture_output=True, text=True, check=True)

    def check(self, environment=None, cwd=None):
        return subprocess.run(['/bin/sh', str(REPOSITORY / 'home/.config/git/hooks/check-public-document-privacy')],
                              cwd=cwd or self.root, env={**self.environment, **(environment or {})},
                              capture_output=True, text=True, timeout=10)

    def test_special_filenames_are_inspected_without_git_display_quoting(self):
        for name in ['手順.md', 'quote"name.md', 'tab\tname.md', 'line\nname.md', '[glob].md', ':colon.md']:
            with self.subTest(name=name):
                path = self.root / name
                path.write_text('/Users/example/private/project\n')
                self.git('--literal-pathspecs', 'add', '--', name)
                result = self.check()
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn('/Users/example/private/project', result.stderr)
                self.git('--literal-pathspecs', 'rm', '--cached', '--', name)
                path.unlink()

    def test_only_added_markdown_lines_are_checked(self):
        path = self.root / '手順.md'
        path.write_text('/Users/example/private/old\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'old content')
        path.write_text('/Users/example/private/old\nsafe addition\n')
        (self.root / 'ignored.txt').write_text('/home/example/private\n')
        self.git('add', '.')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        path.write_text('/Users/example/private/old\nsafe addition\n/home/example/new\n')
        self.git('add', '.')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('手順.md:3:/home/example/new', result.stderr)
        self.assertNotIn('/Users/example/private/old', result.stderr)

    def test_git_failure_cannot_be_reported_as_no_findings(self):
        binary = self.root / 'bin'
        binary.mkdir()
        git = binary / 'git'
        git.write_text('#!/bin/sh\nexit 42\n')
        git.chmod(0o755)
        result = self.check({'PATH': str(binary) + os.pathsep + self.environment.get('PATH', '')})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Git inspection failed', result.stderr)

    def test_failed_per_file_diff_is_not_hidden(self):
        binary = self.root / 'bin'
        binary.mkdir()
        git = binary / 'git'
        git.write_text("#!/bin/sh\nif [ \"$1\" = --literal-pathspecs ]; then exit 42; fi\nprintf 'guide.md\\0'\n")
        git.chmod(0o755)
        result = self.check({'PATH': str(binary) + os.pathsep + self.environment.get('PATH', '')})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Git inspection failed', result.stderr)

    def test_textconv_cannot_hide_the_actual_staged_content(self):
        (self.root / '.gitattributes').write_text('*.md diff=fixture\n')
        (self.root / 'guide.md').write_text('/Users/example/private/project\n')
        self.git('config', 'diff.fixture.textconv', 'echo safe')
        self.git('add', '.')
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('/Users/example/private/project', result.stderr)


if __name__ == '__main__':
    unittest.main()
