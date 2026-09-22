"""Agent operations through the public binary; APM is the external boundary."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', REPOSITORY / 'cli/target'))
APM = r"""
import json, os, sys
from pathlib import Path
root = Path.cwd()
action = sys.argv[1]
if action == os.environ.get('APM_FAIL_ACTION'):
    sys.exit(42)
manifest = root / 'apm.yml'
config = json.loads(manifest.read_text())
deps = config['dependencies']['apm']
if action == 'install':
    for value in sys.argv[2:]:
        if value.startswith('./') and value not in deps:
            deps.append(value)
elif action == 'uninstall':
    deps[:] = [value for value in deps if value not in sys.argv[2:]]
elif action == 'compile':
    (root / 'generated.txt').write_text('\n'.join(deps))
else:
    raise RuntimeError('unsupported APM operation')
manifest.write_text(json.dumps(config))
(root / 'apm.lock.yaml').write_text('generated_at: new\nvalue: 1\n')
"""


class AgentContractTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.repo = self.root / 'repo'
        self.home = self.repo / 'home'
        self.source = self.home / '.apm/skills/example/SKILL.md'
        self.source.parent.mkdir(parents=True)
        self.source.write_text('# owned source')
        (self.repo / 'flake.nix').write_text('{}')
        (self.home / 'apm.yml').write_text(json.dumps({'dependencies': {'apm': ['./.apm/skills/example']}}))
        (self.home / 'apm.lock.yaml').write_text('generated_at: old\nvalue: 1\n')
        (self.home / 'generated.txt').write_text('./.apm/skills/example')
        binary = self.root / 'bin'
        binary.mkdir()
        apm = binary / 'apm'
        apm.write_text('#!' + sys.executable + '\n' + APM)
        apm.chmod(0o755)
        mise = binary / 'mise'
        mise.write_text('#!/bin/sh\n[ "$1" = which ] && [ "$2" = apm ] || exit 2\nprintf "%s\\n" ' + shlex.quote(str(apm)) + '\n')
        mise.chmod(0o755)
        self.env = {**os.environ, 'HOME': str(self.root), 'DOTFILES_DIR': str(self.repo),
                    'PATH': str(binary) + os.pathsep + os.environ.get('PATH', '')}

    def invoke(self, *arguments, **environment):
        return subprocess.run([str(TARGET / 'debug/dotfiles'), 'agents', *arguments],
                              env={**self.env, **environment}, capture_output=True, text=True, timeout=15)

    def test_remove_local_updates_declarations_and_generated_resources_before_removing_source(self):
        result = self.invoke('remove-local', 'example')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.source.exists())
        self.assertEqual(json.loads((self.home / 'apm.yml').read_text())['dependencies']['apm'], [])
        self.assertEqual((self.home / 'generated.txt').read_text(), '')
        self.assertEqual((self.home / 'apm.lock.yaml').read_text(), 'generated_at: old\nvalue: 1\n')

    def test_compile_failure_keeps_owned_source_and_previous_generated_resources(self):
        result = self.invoke('remove-local', 'example', APM_FAIL_ACTION='compile')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.source.read_text(), '# owned source')
        self.assertEqual((self.home / 'generated.txt').read_text(), './.apm/skills/example')
        self.assertEqual((self.home / 'apm.lock.yaml').read_text(), 'generated_at: old\nvalue: 1\n')

    def test_failed_uninstall_keeps_declaration_source_and_generated_resources(self):
        before = (self.home / 'apm.yml').read_bytes()
        result = self.invoke('remove-local', 'example', APM_FAIL_ACTION='uninstall')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.home / 'apm.yml').read_bytes(), before)
        self.assertTrue(self.source.exists())
        self.assertEqual((self.home / 'generated.txt').read_text(), './.apm/skills/example')

    def test_remote_uninstall_cannot_delete_local_source(self):
        for target in ('example', './.apm/skills/example', '.apm/skills/example'):
            result = self.invoke('uninstall', target)
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(self.source.exists())


if __name__ == '__main__':
    unittest.main()
