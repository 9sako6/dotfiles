import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import unittest
from unittest import mock

import launcher


@unittest.skipUnless(os.environ.get("DOTFILES_TEST_OPENCODE"), "set DOTFILES_TEST_OPENCODE to the Nix OpenCode binary")
class OpenCodeTests(unittest.TestCase):
    def test_actual_client_ignores_global_project_and_inline_cloud_configuration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project = root / "project"
            project.mkdir()
            malicious = {"model": "openai/fixture", "plugin": ["https://example.invalid/fixture"], "mcp": {"cloud": {"type": "remote", "url": "https://example.invalid/mcp"}}}
            (project / "opencode.json").write_text(json.dumps(malicious))
            (project / ".agents/skills/external").mkdir(parents=True)
            (project / ".agents/skills/external/SKILL.md").write_text("---\nname: external\ndescription: fixture\n---\nfixture")
            global_config = root / "cloud-config"
            (global_config / "opencode").mkdir(parents=True)
            (global_config / "opencode/opencode.json").write_text(json.dumps(malicious))
            with mock.patch.dict(os.environ, {
                "XDG_CONFIG_HOME": str(global_config),
                "OPENCODE_CONFIG_CONTENT": json.dumps(malicious),
                "OPENCODE_CONFIG": str(project / "opencode.json"),
            }):
                env = launcher.clean_environment(root / "isolated")
            expected = launcher.profile("fixture", 12345, "fixture-token")
            env["OPENCODE_CONFIG_CONTENT"] = json.dumps(expected)
            command = launcher.sandbox(12345) + [os.environ["DOTFILES_TEST_OPENCODE"]]
            result = subprocess.run(command + ["debug", "config"], env=env, cwd=project, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            launcher.verify_profile(json.loads(result.stdout), expected)
            skills = subprocess.run(command + ["debug", "skill"], env=env, cwd=project, capture_output=True, text=True, timeout=30)
            self.assertEqual(skills.returncode, 0, skills.stderr)
            self.assertTrue(all(skill["location"] == "<built-in>" for skill in json.loads(skills.stdout)))

    def test_inference_failure_stays_on_the_local_provider(self):
        calls = []
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                calls.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
                body = json.dumps({"error": {"message": "fixture inference error", "type": "invalid_request_error"}}).encode()
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def log_message(self, *args):
                pass
        with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server, tempfile.TemporaryDirectory() as temporary:
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            port = server.server_address[1]
            env = launcher.clean_environment(Path(temporary))
            env["OPENCODE_CONFIG_CONTENT"] = json.dumps(launcher.profile("fixture", port, "fixture-token"))
            command = launcher.sandbox(port) + [os.environ["DOTFILES_TEST_OPENCODE"], "run", "--format", "json", "Respond with the word fixture."]
            try:
                result = subprocess.run(command, env=env, cwd=temporary, capture_output=True, text=True, timeout=45)
                self.assertTrue(calls, result.stderr + result.stdout)
                self.assertTrue(all(call["model"] == "fixture" for call in calls))
                self.assertIn("fixture inference error", result.stdout + result.stderr)
            finally:
                server.shutdown()
                worker.join()


if __name__ == "__main__":
    unittest.main()
