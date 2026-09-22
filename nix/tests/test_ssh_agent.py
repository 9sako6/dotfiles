"""Use real SSH agents to verify socket recovery and process ownership."""
import concurrent.futures
import contextlib
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]


class SshAgentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='dot-ssh-', dir='/tmp')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.home = self.root / 'home'
        (self.home / '.ssh').mkdir(parents=True, mode=0o700)
        self.socket = self.home / '.ssh/agent.sock'
        self.started = self.root / 'started'
        agent = shutil.which('ssh-agent')
        self.assertIsNotNone(agent, 'SSH agent must be available in the contract-test environment')
        binary = self.root / 'bin'
        binary.mkdir()
        wrapper = binary / 'ssh-agent'
        wrapper.write_text('''#!/bin/sh
set -eu
output="$("$REAL_SSH_AGENT" "$@")"
printf '%s\\n' "$output" >> "$AGENT_STARTS"
printf '%s\\n' "$output"
''')
        wrapper.chmod(0o755)
        self.environment = {**os.environ, 'HOME': str(self.home), 'SSH_AUTH_SOCK': str(self.socket),
                            'PATH': str(binary) + os.pathsep + os.environ.get('PATH', ''),
                            'REAL_SSH_AGENT': agent, 'AGENT_STARTS': str(self.started)}
        self.addCleanup(self.stop_agents)

    def pids(self):
        text = self.started.read_text() if self.started.exists() else ''
        return {int(pid) for pid in re.findall(r'SSH_AGENT_PID=(\d+)', text)}

    def stop_agents(self):
        for pid in self.pids():
            with contextlib.suppress(ProcessLookupError):
                os.kill(pid, signal.SIGTERM)

    def setup_shell(self, **environment):
        return subprocess.run(['zsh', '-f', '-c', '. "$1"; printf "%s" "${SSH_AUTH_SOCK:-}"',
                               'ssh-fixture', str(REPOSITORY / 'home/.zsh.d/ssh-agent.zsh')],
                              env={**self.environment, **environment},
                              capture_output=True, text=True, timeout=15)

    def assert_connected(self):
        result = subprocess.run(['ssh-add', '-l'], env=self.environment,
                                capture_output=True, text=True, timeout=5)
        self.assertIn(result.returncode, [0, 1], result.stderr)

    def test_unreachable_owned_socket_is_replaced_by_a_live_agent(self):
        with socket.socket(socket.AF_UNIX) as stale:
            stale.bind(str(self.socket))
        result = self.setup_shell()
        self.assertEqual(result.stdout, str(self.socket))
        self.assertEqual(result.stderr, '')
        self.assert_connected()
        self.assertEqual(len(self.pids()), 1)

    def test_repeated_initialization_keeps_the_live_keyless_agent(self):
        for _ in range(3):
            result = self.setup_shell()
            self.assertEqual(result.stderr, '')
            self.assert_connected()
        self.assertEqual(len(self.pids()), 1)

    def test_parallel_initialization_creates_one_reachable_agent(self):
        with socket.socket(socket.AF_UNIX) as stale:
            stale.bind(str(self.socket))
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as workers:
            results = list(workers.map(lambda _: self.setup_shell(), range(6)))
        for result in results:
            self.assertEqual(result.stderr, '')
            self.assertEqual(result.stdout, str(self.socket))
        self.assert_connected()
        self.assertEqual(len(self.pids()), 1)

    def test_externally_supplied_socket_is_never_replaced(self):
        external = self.root / 'external.sock'
        with socket.socket(socket.AF_UNIX) as stale:
            stale.bind(str(external))
        identity = external.stat().st_ino
        result = self.setup_shell(SSH_AUTH_SOCK=str(external))
        self.assertEqual(result.stdout, str(external))
        self.assertEqual(external.stat().st_ino, identity)
        self.assertFalse(self.socket.exists())
        self.assertEqual(self.pids(), set())

    def test_non_socket_and_symlink_paths_are_preserved(self):
        target = self.root / 'unowned'
        target.write_text('keep')
        for symlink in [False, True]:
            with self.subTest(symlink=symlink):
                self.socket.unlink(missing_ok=True)
                if symlink:
                    self.socket.symlink_to(target)
                else:
                    self.socket.write_text('keep')
                result = self.setup_shell()
                self.assertIn('refusing to replace', result.stderr)
                self.assertEqual(self.socket.is_symlink(), symlink)
                self.assertEqual(self.socket.read_text(), 'keep')
                self.assertEqual(target.read_text(), 'keep')
                self.assertEqual(self.pids(), set())

    def test_loaded_identity_survives_reinitialization(self):
        self.setup_shell()
        key = self.root / 'fixture-key'
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(key)],
                       env=self.environment, check=True, capture_output=True, timeout=15)
        subprocess.run(['ssh-add', str(key)], env=self.environment, check=True,
                       capture_output=True, timeout=5)
        before = subprocess.check_output(['ssh-add', '-L'], env=self.environment)
        pids = self.pids()
        self.setup_shell()
        self.assertEqual(subprocess.check_output(['ssh-add', '-L'], env=self.environment), before)
        self.assertEqual(self.pids(), pids)


if __name__ == '__main__':
    unittest.main()
