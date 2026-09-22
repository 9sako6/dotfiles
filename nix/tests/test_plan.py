"""Public plan/apply contracts. Only Nix and privilege effects are replaced.

The real CLI, Git fingerprinting, confirmation, build orchestration, shell
activation transaction and home copy code all run. Nothing under /etc or /nix
is modified; the backend owns the platform paths and supplies a temporary host.
"""
import json
import os
from pathlib import Path
import select
import shlex
import subprocess
import sys
import tempfile
import time
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", REPOSITORY / "cli/target"))

NIX = r"""
import json, os, shutil, subprocess, sys
from pathlib import Path
s = Path(os.environ['CONTRACT_STATE'])
r = Path(os.environ['DOTFILES_DIR'])
a = sys.argv[1:]
a = a[2:] if a[:1] == ['--extra-experimental-features'] else a
source = s / 'source'
old = s / 'old'
new = old if os.environ.get('UNCHANGED') else s / 'new'

def inventory():
    return {'schemaVersion': 4, 'source': str(source),
            'packages': [{'name': 'fixture', 'manager': 'Nix',
                          'declared': '1' if os.environ.get('UNCHANGED') else '2'}],
            'system': [], 'services': [], 'tools': [], 'skills': [], 'timeZone': 'UTC',
            'localllm': {'enabled': False, 'default_model': None}}

if a[:2] == ['flake', 'metadata']:
    source.mkdir(exist_ok=True)
    names = subprocess.check_output(['git', '-C', str(r), 'ls-files', '-z']).split(b'\0')
    for raw in filter(None, names):
        name = os.fsdecode(raw)
        destination = source / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(r / name, destination)
    print(json.dumps({'path': str(source), 'locked': {'narHash': 'sha256-' + 'A' * 43 + '='}}))
elif a[:2] == ['store', 'add-path']:
    host = s / 'host'
    shutil.copytree(a[-1], host, dirs_exist_ok=True)
    print(host)
elif a[:1] == ['eval']:
    if '--file' in a:
        print(json.dumps({'errors': [], 'config': {'copy': ['managed/settings'], 'private': {'path': None}}}))
    else:
        print(json.dumps(inventory()))
elif a[:1] == ['build']:
    if any(arg.endswith('#inventory') for arg in a):
        data = s / 'inventory.json'
        data.write_text(json.dumps(inventory()))
        print(json.dumps([{'outputs': {'out': str(data)}}]))
    elif '--dry-run' in a:
        print(json.dumps([{'drvPath': str(s / (name + '.drv')), 'outputs': {'out': str(path)}}
                          for name, path in [('system', new), ('brewfile', s / 'brewfile')]]))
    elif '--out-link' in a:
        link = Path(a[a.index('--out-link') + 1])
        if link.name == 'input':
            sys.exit(0)
        (s / 'built').touch()
        (new / 'sw/bin').mkdir(parents=True, exist_ok=True)
        shutil.copy2(s / 'rebuild', new / 'sw/bin/darwin-rebuild')
        (new / 'dotfiles-inventory.json').write_text(json.dumps(inventory()))
        (new / 'darwin-version.json').write_text('{"configurationRevision":"fixture"}')
        link.symlink_to(new)
        if os.environ.get('DRIFT') == 'input':
            (r / 'home/managed/settings').write_text('edited during build')
        elif os.environ.get('DRIFT') == 'generation':
            (s / 'current').unlink()
            (s / 'current').symlink_to(s / 'other')
    else:
        raise RuntimeError('unsupported Nix build operation')
else:
    raise RuntimeError('unsupported Nix operation')
"""


class PlanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.repo = self.root / 'repo'
        self.home = self.root / 'home'
        self.state = self.root / 'state'
        for directory in [self.repo / 'bin', self.repo / 'nix', self.repo / 'home/managed',
                          self.home / 'managed', self.state / 'nix/bin', self.state / 'old', self.state / 'other']:
            directory.mkdir(parents=True)
        for name, content in {
            'flake.nix': '{}', 'flake.lock': '{}', 'nix/host-flake.nix': '{}',
            '.gitignore': 'dotfiles.local.toml\n', 'dotfiles.toml': 'copy = ["managed/settings"]\n',
            'home/apm.yml': 'dependencies:\n  apm: []\n', 'home/managed/settings': 'desired',
        }.items():
            (self.repo / name).write_text(content)
        (self.home / 'managed/settings').write_text('current')
        (self.home / 'history').write_text('keep')
        before = {'schemaVersion': 4, 'source': str(self.repo),
                  'packages': [{'name': 'fixture', 'manager': 'Nix', 'declared': '1'}],
                  'system': [], 'services': [], 'tools': [], 'skills': [], 'timeZone': 'UTC',
                  'localllm': {'enabled': False, 'default_model': None}}
        (self.state / 'old/dotfiles-inventory.json').write_text(json.dumps(before))
        (self.state / 'old/darwin-version.json').write_text('{"configurationRevision":"previous"}')
        (self.state / 'current').symlink_to(self.state / 'old')
        self.executable(self.state / 'nix/bin/nix', '#!' + sys.executable + '\n' + NIX)
        self.executable(self.state / 'nix/bin/nix-env', '#!/bin/sh\n: > "$CONTRACT_STATE/profile-set"\n')
        self.executable(self.state / 'sudo', '''#!/bin/sh
set -eu
case "${1:-}" in --user=*) shift; [ "$1" = -- ]; shift ;; esac
exec "$@"
''')
        self.executable(self.state / 'rebuild', '''#!/bin/sh
set -eu
[ ! -e "$CONTRACT_STATE/record" ]
[ "$(cat "$HOME/managed/settings")" = current ]
[ "${FAIL_STAGE:-}" != activate ] || exit 42
rm "$CONTRACT_STATE/current"
ln -s "$CONTRACT_STATE/new" "$CONTRACT_STATE/current"
: > "$CONTRACT_STATE/activated"
if [ "${FAIL_STAGE:-}" = copy ]; then
  mv "$HOME/managed" "$HOME/untouched-managed"
  ln -s "$HOME/untouched-managed" "$HOME/managed"
fi
''')
        paths = shlex.quote(json.dumps({'sourceRecord': str(self.state / 'record'),
                                      'currentGeneration': str(self.state / 'current')}))
        self.executable(self.repo / 'bin/system-backend.sh', '''#!/bin/sh
set -eu
. "$CONTRACT_IMPLEMENTATION/lib/install-system.sh"
operation="$1"; shift
case "$operation" in
  paths) printf '%s\\n' ''' + paths + ''' ;;
  require-nix|ensure-nix) printf '%s\\n' "$CONTRACT_STATE/nix/bin/nix" ;;
  preview) printf '%s\\n' 'fixture native diff' ;;
  activate)
    nix="$1"; user="$2"; system="$3"; expected="$4"; desired="$5"; shift 5
    install_system_apply_built_system "$CONTRACT_STATE/sudo" /usr/bin/env "$nix" "$user" "$system" \\
      "$CONTRACT_STATE/record" "$expected" "$desired" "$@"
    ;;
  *) exit 2 ;;
esac
''')
        self.env = {**os.environ, 'HOME': str(self.home), 'DOTFILES_DIR': str(self.repo),
                    'CONTRACT_STATE': str(self.state), 'CONTRACT_IMPLEMENTATION': str(REPOSITORY),
                    'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1'}
        self.git('init', '--quiet', '-b', 'master')
        self.git('add', '.')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                 '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def executable(self, path, content):
        path.write_text(content)
        path.chmod(0o755)

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.repo), *args], env=self.env,
                              check=True, capture_output=True, text=True)

    def invoke(self, operation, answer='', **environment):
        return subprocess.run([str(TARGET / 'debug/dotfiles'), operation],
                              env={**self.env, **environment}, input=answer,
                              capture_output=True, text=True, timeout=30)

    def assert_untouched(self):
        self.assertFalse((self.state / 'activated').exists())
        self.assertFalse((self.state / 'record').is_symlink())
        self.assertEqual((self.state / 'current').resolve(), self.state / 'old')
        self.assertEqual((self.home / 'managed/settings').read_text(), 'current')
        self.assertEqual((self.home / 'history').read_text(), 'keep')

    def test_plan_shows_changes_without_building_or_mutating_the_host(self):
        result = self.invoke('plan')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('fixture', result.stdout)
        self.assertNotIn('Type yes', result.stdout)
        self.assertFalse((self.state / 'built').exists())
        self.assert_untouched()

    def test_declining_apply_does_not_build_or_activate(self):
        result = self.invoke('apply', 'no\n')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('Type yes', result.stdout)
        self.assertFalse((self.state / 'built').exists())
        self.assert_untouched()

    def test_approval_activates_then_copies_then_records_success(self):
        result = self.invoke('apply', 'yes\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.state / 'built').exists())
        self.assertTrue((self.state / 'activated').exists())
        self.assertEqual((self.state / 'current').resolve(), self.state / 'new')
        self.assertEqual((self.state / 'record').readlink(), self.repo / 'flake.nix')
        self.assertEqual((self.home / 'managed/settings').read_text(), 'desired')
        self.assertEqual((self.home / 'history').read_text(), 'keep')

    def test_unchanged_apply_neither_prompts_nor_builds(self):
        (self.repo / 'home/managed/settings').write_text('current')
        result = self.invoke('apply', UNCHANGED='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, '')
        self.assertFalse((self.state / 'built').exists())
        self.assert_untouched()

    def test_edit_between_preview_and_approval_is_not_activated(self):
        process = subprocess.Popen([str(TARGET / 'debug/dotfiles'), 'apply'], env=self.env,
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            data = b''
            deadline = time.monotonic() + 15
            while b'Type yes:' not in data:
                self.assertLess(time.monotonic(), deadline, data)
                if select.select([process.stdout], [], [], 0.1)[0]:
                    chunk = os.read(process.stdout.fileno(), 65536)
                    self.assertTrue(chunk, data)
                    data += chunk
            (self.repo / 'home/managed/settings').write_text('changed after preview')
            _, error = process.communicate(b'yes\n', timeout=10)
            self.assertEqual(process.returncode, 1, error)
            self.assertIn(b'inputs changed', error)
            self.assertFalse((self.state / 'built').exists())
            self.assert_untouched()
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            for stream in [process.stdin, process.stdout, process.stderr]:
                stream.close()

    def test_edit_during_build_does_not_activate(self):
        result = self.invoke('apply', 'yes\n', DRIFT='input')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('inputs changed', result.stderr)
        self.assertTrue((self.state / 'built').exists())
        self.assert_untouched()

    def test_generation_switch_during_build_does_not_activate(self):
        result = self.invoke('apply', 'yes\n', DRIFT='generation')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('active generation changed', result.stderr)
        self.assertFalse((self.state / 'activated').exists())
        self.assertFalse((self.state / 'record').is_symlink())

    def test_activation_failure_does_not_copy_or_record_success(self):
        result = self.invoke('apply', 'yes\n', FAIL_STAGE='activate')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertTrue((self.state / 'built').exists())
        self.assert_untouched()

    def test_copy_failure_after_activation_does_not_record_success(self):
        result = self.invoke('apply', 'yes\n', FAIL_STAGE='copy')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertTrue((self.state / 'activated').exists())
        self.assertFalse((self.state / 'record').is_symlink())
        self.assertEqual((self.home / 'untouched-managed/settings').read_text(), 'current')
        self.assertEqual((self.home / 'history').read_text(), 'keep')


if __name__ == '__main__':
    unittest.main()
