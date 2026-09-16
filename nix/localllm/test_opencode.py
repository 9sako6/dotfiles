import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest import mock

import launcher


@unittest.skipUnless(os.environ.get("DOTFILES_TEST_OPENCODE"), "set DOTFILES_TEST_OPENCODE to the Nix OpenCode binary")
class OpenCodeTests(unittest.TestCase):
    def test_client_runs_user_commands_and_fetches_web_content(self):
        calls = []
        fetched = []

        class WebHandler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                fetched.append(self.path)
                body = b"local-web-fixture"
                self.send_response(200)
                self.send_header("Content-Type", "text/plain")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        class ModelHandler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                body = json.dumps({"data": [{"id": "fixture"}]}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                calls.append(body)
                if body.get("tools") and not any(message["role"] == "tool" for message in body["messages"]):
                    delta = {"role": "assistant", "content": "ユーザー環境とWeb取得を確認します。", "tool_calls": [
                        {"index": 0, "id": "call_command", "type": "function", "function": {
                            "name": "bash", "arguments": json.dumps({"command": "local-fixture", "description": "Check user tool environment"}),
                        }},
                        {"index": 1, "id": "call_fetch", "type": "function", "function": {
                            "name": "webfetch", "arguments": json.dumps({"url": web_url, "format": "text"}),
                        }},
                    ]}
                    reason = "tool_calls"
                else:
                    delta = {"role": "assistant", "content": "fixture complete"}
                    reason = "stop"
                events = [
                    {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "fixture", "choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                    {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "fixture", "choices": [{"index": 0, "delta": {}, "finish_reason": reason}], "usage": {"prompt_tokens": 8000, "completion_tokens": 100, "total_tokens": 8100}},
                ]
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                for event in events:
                    self.wfile.write(f"data: {json.dumps(event)}\n\n".encode())
                self.wfile.write(b"data: [DONE]\n\n")

            def log_message(self, *args):
                pass

        with http.server.ThreadingHTTPServer(("127.0.0.1", 0), WebHandler) as web, http.server.ThreadingHTTPServer(("127.0.0.1", 0), ModelHandler) as model, tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "state").mkdir()
            home = root / "user"
            home.mkdir()
            binary = root / "bin"
            binary.mkdir()
            tool = binary / "local-fixture"
            tool.write_text('#!/bin/sh\nprintf "home=%s setting=%s\\n" "$HOME" "$LOCALLLM_FIXTURE"\n')
            tool.chmod(0o755)
            web_url = f"http://127.0.0.1:{web.server_address[1]}/fixture"
            workers = [threading.Thread(target=server.serve_forever, daemon=True) for server in (web, model)]
            for worker in workers:
                worker.start()
            environment = dict(os.environ, HOME=str(home), PATH=str(binary) + os.pathsep + os.defpath, SHELL="/bin/sh", LOCALLLM_FIXTURE="inherited", PYTHONPATH=str(Path(launcher.__file__).parent))
            script = "import launcher,sys,os; from pathlib import Path; sys.exit(launcher.run_client({'opencode':sys.argv[1],'goal_plugin':os.environ.get('DOTFILES_TEST_GOAL_PLUGIN')},Path(sys.argv[2]),sys.argv[3],'fixture',int(sys.argv[4]),'fixture-token',['run','--format','json','Run the fixture tools.']))"
            try:
                result = subprocess.run([sys.executable, "-c", script, os.environ["DOTFILES_TEST_OPENCODE"], str(root / "state"), temporary, str(model.server_address[1])], env=environment, capture_output=True, text=True, timeout=45)
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertGreaterEqual(len(calls), 2, result.stdout)
                self.assertLessEqual(len(calls), 4, result.stdout)
                tool_calls = [call for call in calls if call.get("tools")]
                self.assertTrue(tool_calls, result.stdout)
                tools = {tool["function"]["name"] for tool in tool_calls[0]["tools"]}
                self.assertTrue({"bash", "webfetch", "websearch"}.issubset(tools), tools)
                self.assertIn("ユーザー環境とWeb取得を確認します。", result.stdout)
                system = "\n".join(message["content"] for message in tool_calls[0]["messages"] if message["role"] == "system")
                self.assertIn(Path(launcher.__file__).with_name("progress-instructions.md").read_text().strip(), system)
                if os.environ.get("DOTFILES_TEST_GOAL_PLUGIN"):
                    self.assertTrue({"create_goal", "get_goal", "update_goal"}.issubset(tools), tools)
                outputs = json.dumps([message for call in tool_calls for message in call["messages"] if message["role"] == "tool"])
                self.assertIn(f"home={home} setting=inherited", outputs)
                self.assertIn("local-web-fixture", outputs)
                self.assertEqual(fetched, ["/fixture"])
                self.assertTrue(all(call["model"] == "fixture" for call in calls))
            finally:
                for server in (web, model):
                    server.shutdown()
                for worker in workers:
                    worker.join()

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
                env = launcher.client_environment(root / "isolated")
            expected = launcher.profile("fixture", 12345, "fixture-token")
            env["OPENCODE_CONFIG_CONTENT"] = json.dumps(expected)
            command = [os.environ["DOTFILES_TEST_OPENCODE"]]
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
            env = launcher.client_environment(Path(temporary))
            env["OPENCODE_CONFIG_CONTENT"] = json.dumps(launcher.profile("fixture", port, "fixture-token"))
            command = [os.environ["DOTFILES_TEST_OPENCODE"], "run", "--format", "json", "Respond with the word fixture."]
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
