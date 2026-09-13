import importlib.util
import json
import os
from pathlib import Path
import socket
import signal
import time
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import launcher


class LauncherTests(unittest.TestCase):
    def test_owned_child_stops_without_terminating_other_process(self):
        other = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        try:
            with launcher.owned_process([sys.executable, "-c", "import time; time.sleep(60)"]) as child:
                self.assertIsNone(child.poll())
            self.assertIsNotNone(child.poll())
            self.assertIsNone(other.poll())
        finally:
            other.terminate()
            other.wait()

    def test_sigterm_cleans_up_the_owned_server(self):
        with tempfile.TemporaryDirectory() as directory:
            pidfile = Path(directory) / "child.pid"
            script = "import launcher,sys,time; from pathlib import Path; launcher.install_signals();\nwith launcher.owned_process([sys.executable,'-c','import time; time.sleep(60)']) as p:\n Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(60)"
            environment = dict(os.environ, PYTHONPATH=str(Path(launcher.__file__).parent))
            process = subprocess.Popen([sys.executable, "-c", script, str(pidfile)], env=environment, stderr=subprocess.DEVNULL)
            try:
                for _ in range(100):
                    if pidfile.exists():
                        break
                    time.sleep(0.02)
                child = int(pidfile.read_text())
                process.send_signal(signal.SIGTERM)
                process.wait(timeout=20)
                with self.assertRaises(ProcessLookupError):
                    os.kill(child, 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()

    def test_unavailable_server_fails_before_starting_client(self):
        with tempfile.TemporaryDirectory() as directory, socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
            with mock.patch("launcher.subprocess.run") as start:
                with self.assertRaisesRegex(RuntimeError, "no fallback"):
                    launcher.run_client({}, Path(directory), directory, "fixture", port, "token", [])
                start.assert_not_called()

    def test_merged_cloud_and_extension_settings_are_rejected(self):
        expected = launcher.profile("fixture", 12345, "token")
        launcher.verify_profile(expected, expected)
        for key, value in [("model", "cloud/model"), ("small_model", "cloud/small"), ("plugin", ["external"]), ("mcp", {"external": {}}), ("share", "auto")]:
            changed = {**expected, key: value}
            with self.assertRaises(RuntimeError):
                launcher.verify_profile(changed, expected)

    def test_environment_excludes_inherited_credentials_and_inline_config(self):
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "test-secret", "HTTP_PROXY": "test-proxy", "OPENCODE_CONFIG_CONTENT": "test-config"}):
                environment = launcher.clean_environment(Path(directory))
            self.assertNotIn("OPENAI_API_KEY", environment)
            self.assertNotIn("HTTP_PROXY", environment)
            self.assertNotIn("OPENCODE_CONFIG_CONTENT", environment)
            self.assertEqual(environment["OPENCODE_DISABLE_EXTERNAL_SKILLS"], "1")
            self.assertNotEqual(environment["XDG_DATA_HOME"], os.environ.get("XDG_DATA_HOME"))

    @unittest.skipUnless(sys.platform == "darwin", "macOS network sandbox")
    def test_network_policy_is_inherited_by_tool_child(self):
        with socket.socket() as permitted, socket.socket() as forbidden:
            permitted.bind(("127.0.0.1", 0))
            forbidden.bind(("127.0.0.1", 0))
            permitted.listen()
            forbidden.listen()
            allowed = permitted.getsockname()[1]
            denied = forbidden.getsockname()[1]
            script = "import socket,sys; s=socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=1); s.close()"
            child = "import subprocess,sys; sys.exit(subprocess.call([sys.executable,'-c',sys.argv[1],sys.argv[2]]))"
            base = launcher.sandbox(allowed) + [sys.executable, "-c", child, script]
            self.assertEqual(subprocess.run(base + [str(allowed)], capture_output=True).returncode, 0)
            self.assertNotEqual(subprocess.run(base + [str(denied)], capture_output=True).returncode, 0)
            forbidden.settimeout(0.1)
            with self.assertRaises(TimeoutError):
                forbidden.accept()


if __name__ == "__main__":
    unittest.main()
