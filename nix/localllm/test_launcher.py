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
    def test_chat_server_logs_do_not_overwrite_client_output(self):
        with tempfile.TemporaryDirectory() as directory:
            script = "import launcher,os,sys; from pathlib import Path;\nwith launcher.server_process([sys.executable,'-c',\"import sys; print('server stdout'); print('server stderr', file=sys.stderr); assert sys.stdin.read() == ''\"],os.environ,Path(sys.argv[1]),'chat') as server:\n assert server.wait() == 0\nprint('client output')"
            result = subprocess.run([sys.executable, "-c", script, directory], env=dict(os.environ, PYTHONPATH=str(Path(launcher.__file__).parent)), capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("client output", result.stdout)
            self.assertNotIn("server stdout", result.stdout + result.stderr)
            self.assertNotIn("server stderr", result.stdout + result.stderr)
            log = Path(directory) / "server.log"
            self.assertIn("server stdout", log.read_text())
            self.assertIn("server stderr", log.read_text())
            self.assertEqual(log.stat().st_mode & 0o777, 0o600)

    def test_standalone_server_keeps_terminal_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            script = "import launcher,os,sys; from pathlib import Path;\nwith launcher.server_process([sys.executable,'-c',\"import sys; print('server stdout'); print('server stderr', file=sys.stderr)\"],os.environ,Path(sys.argv[1]),'serve') as server:\n assert server.wait() == 0"
            result = subprocess.run([sys.executable, "-c", script, directory], env=dict(os.environ, PYTHONPATH=str(Path(launcher.__file__).parent)), capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("server stdout", result.stdout)
            self.assertIn("server stderr", result.stderr)
            self.assertFalse((Path(directory) / "server.log").exists())

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

    def test_server_environment_excludes_inherited_credentials_and_inline_config(self):
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "test-secret", "HTTP_PROXY": "test-proxy", "OPENCODE_CONFIG_CONTENT": "test-config"}):
                environment = launcher.clean_environment(Path(directory))
            self.assertNotIn("OPENAI_API_KEY", environment)
            self.assertNotIn("HTTP_PROXY", environment)
            self.assertNotIn("OPENCODE_CONFIG_CONTENT", environment)
            self.assertEqual(environment["OPENCODE_DISABLE_EXTERNAL_SKILLS"], "1")
            self.assertNotEqual(environment["XDG_DATA_HOME"], os.environ.get("XDG_DATA_HOME"))

    def test_client_preserves_tool_environment_without_inheriting_opencode_overrides(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / "user"
            with mock.patch.dict(os.environ, {
                "HOME": str(home),
                "PATH": "/fixture/bin:/usr/bin:/bin",
                "SHELL": "/bin/zsh",
                "LOCALLLM_FIXTURE": "tool-setting",
                "OPENCODE_CONFIG": "/fixture/cloud.json",
                "OPENCODE_CONFIG_CONTENT": "cloud-config",
                "OPENCODE_WEBSEARCH_PROVIDER": "parallel",
            }):
                environment = launcher.client_environment(Path(directory) / "isolated")
            self.assertEqual(environment["HOME"], str(home))
            self.assertEqual(environment["PATH"], "/fixture/bin:/usr/bin:/bin")
            self.assertEqual(environment["SHELL"], "/bin/zsh")
            self.assertEqual(environment["LOCALLLM_FIXTURE"], "tool-setting")
            self.assertNotIn("OPENCODE_CONFIG", environment)
            self.assertNotIn("OPENCODE_CONFIG_CONTENT", environment)
            self.assertNotIn("OPENCODE_WEBSEARCH_PROVIDER", environment)

    @unittest.skipUnless(sys.platform == "darwin", "macOS network sandbox")
    def test_server_cannot_initiate_outbound_connections(self):
        with socket.socket() as forbidden:
            forbidden.bind(("127.0.0.1", 0))
            forbidden.listen()
            denied = forbidden.getsockname()[1]
            script = "import socket,sys; s=socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=1); s.close()"
            child = "import subprocess,sys; sys.exit(subprocess.call([sys.executable,'-c',sys.argv[1],sys.argv[2]]))"
            base = launcher.server_sandbox() + [sys.executable, "-c", child, script]
            self.assertNotEqual(subprocess.run(base + [str(denied)], capture_output=True).returncode, 0)
            forbidden.settimeout(0.1)
            with self.assertRaises(TimeoutError):
                forbidden.accept()


if __name__ == "__main__":
    unittest.main()
