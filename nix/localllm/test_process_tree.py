import contextlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

import launcher


CHILD = """
import json, os, signal, sys, time
from pathlib import Path
root = Path(sys.argv[1])
signal.signal(signal.SIGTERM, signal.SIG_IGN)
(root / 'child.json').write_text(json.dumps([os.getpid(), os.getpgrp()]))
while True:
    with (root / 'heartbeat').open('a') as output:
        output.write('alive\\n')
    time.sleep(0.02)
"""
LEADER = """
import subprocess, sys, time
subprocess.Popen([sys.executable, '-c', sys.argv[1], sys.argv[2]])
while True:
    time.sleep(60)
"""


class ProcessTreeTests(unittest.TestCase):
    def test_fast_exit_and_normal_termination_are_reaped(self):
        for script in ("pass", "import time; time.sleep(60)"):
            for _ in range(8):
                with launcher.owned_process([sys.executable, "-c", script]) as process:
                    if script == "pass":
                        process.wait(timeout=5)
                self.assertIsNotNone(process.returncode)

    def wait_for_child(self, root):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                child = json.loads((root / 'child.json').read_text())
                if (root / 'heartbeat').stat().st_size:
                    return child
            except (FileNotFoundError, json.JSONDecodeError):
                pass
            time.sleep(0.02)
        self.fail('owned descendant did not start')

    def assert_stopped(self, pid, root):
        deadline = time.monotonic() + 5
        while True:
            result = subprocess.run(['ps', '-o', 'stat=', '-p', str(pid)],
                                    capture_output=True, text=True, check=False)
            state = result.stdout.strip()
            # Orphan zombies can await OS reaping, but must no longer execute.
            if not state or state.startswith('Z'):
                break
            self.assertLess(time.monotonic(), deadline, state)
            time.sleep(0.02)
        before = (root / 'heartbeat').read_bytes()
        time.sleep(0.1)
        self.assertEqual((root / 'heartbeat').read_bytes(), before)

    def test_normal_and_exception_exit_stop_descendants_after_leader_exit(self):
        for fail in (False, True):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                other = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
                group = None
                try:
                    with mock.patch.object(launcher, '_PROCESS_STOP_TIMEOUT', 0.2):
                        try:
                            with launcher.owned_process([sys.executable, '-c', LEADER, CHILD, temporary]) as parent:
                                group = parent.pid
                                child, pgid = self.wait_for_child(root)
                                self.assertEqual(pgid, parent.pid)
                                parent.terminate()
                                parent.wait(timeout=5)
                                if fail:
                                    raise ValueError('original failure')
                        except ValueError as error:
                            self.assertTrue(fail)
                            self.assertEqual(str(error), 'original failure')
                        else:
                            self.assertFalse(fail)
                    self.assert_stopped(child, root)
                    self.assertIsNone(other.poll())
                finally:
                    if group is not None:
                        launcher.signal_process_group(group, signal.SIGKILL)
                    other.terminate()
                    other.wait(timeout=5)

    def test_sigterm_to_launcher_stops_the_whole_owned_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script = """
import launcher, sys, time
launcher._PROCESS_STOP_TIMEOUT = 0.2
launcher.install_signals()
try:
    with launcher.owned_process([sys.executable, '-c', sys.argv[1], sys.argv[2], sys.argv[3]]):
        time.sleep(60)
except KeyboardInterrupt:
    sys.exit(130)
"""
            environment = dict(os.environ, PYTHONPATH=str(Path(launcher.__file__).parent))
            process = subprocess.Popen([sys.executable, '-c', script, LEADER, CHILD, temporary], env=environment)
            group = None
            try:
                child, group = self.wait_for_child(root)
                process.send_signal(signal.SIGTERM)
                self.assertEqual(process.wait(timeout=5), 130)
                self.assert_stopped(child, root)
            finally:
                if group is not None:
                    launcher.signal_process_group(group, signal.SIGKILL)
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
