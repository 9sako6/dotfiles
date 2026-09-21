import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]


class ApplyLockTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.selection = self.root / 'etc/flake.nix'
        self.selection.parent.mkdir()
        self.executable('sudo', '#!/bin/sh\nexec "$@"\n')
        self.executable('lix/bin/nix', '#!/bin/sh\nexit 0\n')
        self.executable('lix/bin/nix-env', '#!/bin/sh\nexit 0\n')
        self.executable('system/sw/bin/darwin-rebuild', '''#!/bin/sh
printf '%s\\n' "$PPID" > "$FIXTURE_ROOT/owner"
if ! mkdir "$FIXTURE_ROOT/active" 2>/dev/null; then
  touch "$FIXTURE_ROOT/overlap"
  exit 1
fi
trap 'rmdir "$FIXTURE_ROOT/active"' EXIT
printf '%s\\n' entered >> "$FIXTURE_ROOT/entries"
if [ -e "$FIXTURE_ROOT/fail" ]; then exit 42; fi
while [ ! -e "$FIXTURE_ROOT/release" ]; do sleep 0.02; done
''')
        self.children = []
        self.addCleanup(self.stop_children)

    def executable(self, path, text):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        path.chmod(0o755)

    def start(self, expected='missing', desired='/fixture/flake.nix'):
        command = ['sh', '-eu', '-c', '. "$1"; shift; install_system_apply_built_system "$@"',
                   'apply-test', str(REPOSITORY / 'lib/install-system.sh'),
                   str(self.root / 'sudo'), '/usr/bin/env', str(self.root / 'lix/bin/nix'),
                   'fixture', str(self.root / 'system'), str(self.selection), expected, desired]
        process = subprocess.Popen(command, env={**os.environ, 'FIXTURE_ROOT': str(self.root)},
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                   start_new_session=True)
        self.children.append(process)
        return process

    def stop_children(self):
        (self.root / 'release').touch()
        for process in self.children:
            try:
                process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate(timeout=5)

    def wait_for(self, path):
        deadline = time.monotonic() + 5
        while not (self.root / path).exists():
            self.assertLess(time.monotonic(), deadline, path)
            time.sleep(0.02)

    def finish(self, process, code):
        output, errors = process.communicate(timeout=5)
        self.assertEqual(process.returncode, code, errors + output)
        return errors

    def test_stale_contents_do_not_allow_two_concurrent_activations(self):
        Path(str(self.selection) + '.apply.lock').write_text('2147483647\n')
        contenders = [self.start() for _ in range(4)]
        self.wait_for('active')
        deadline = time.monotonic() + 5
        while sum(process.poll() is None for process in contenders) != 1:
            self.assertLess(time.monotonic(), deadline, 'contenders did not resolve')
            time.sleep(0.02)
        first = next(process for process in contenders if process.poll() is None)
        for rejected in (process for process in contenders if process is not first):
            _, errors = rejected.communicate(timeout=5)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('system apply is already running', errors)
        self.assertFalse((self.root / 'overlap').exists())
        self.assertEqual((self.root / 'entries').read_text(), 'entered\n')
        self.assertFalse(self.selection.is_symlink())
        (self.root / 'release').touch()
        self.finish(first, 0)
        self.assertEqual(os.readlink(self.selection), '/fixture/flake.nix')
        self.finish(self.start('/fixture/flake.nix', '/next/flake.nix'), 0)
        self.assertEqual(os.readlink(self.selection), '/next/flake.nix')

    def test_failed_activation_releases_the_lock_without_updating_record(self):
        (self.root / 'fail').touch()
        self.finish(self.start(), 42)
        self.assertFalse(self.selection.is_symlink())
        (self.root / 'fail').unlink()
        (self.root / 'release').touch()
        self.finish(self.start(), 0)
        self.assertEqual(os.readlink(self.selection), '/fixture/flake.nix')

    def test_killed_owner_does_not_unlock_a_running_activation_descendant(self):
        first = self.start()
        self.wait_for('active')
        owner = int((self.root / 'owner').read_text())
        os.kill(owner, signal.SIGKILL)
        second = self.start()
        _, errors = second.communicate(timeout=5)
        self.assertNotEqual(second.returncode, 0)
        self.assertIn('system apply is already running', errors)
        self.assertFalse((self.root / 'overlap').exists())
        (self.root / 'release').touch()
        first.communicate(timeout=5)
        self.assertFalse(self.selection.is_symlink())
        self.finish(self.start(), 0)

    def test_symlink_lock_is_rejected_without_modifying_its_target(self):
        outside = self.root / 'outside'
        outside.write_text('keep')
        Path(str(self.selection) + '.apply.lock').symlink_to(outside)
        process = self.start()
        _, errors = process.communicate(timeout=5)
        self.assertNotEqual(process.returncode, 0)
        self.assertIn('cannot open system apply lock', errors)
        self.assertEqual(outside.read_text(), 'keep')
        self.assertFalse((self.root / 'entries').exists())


if __name__ == '__main__':
    unittest.main()
