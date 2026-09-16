import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time
import unittest
import urllib.request

import launcher


@unittest.skipUnless(os.environ.get("DOTFILES_TEST_OPENCODE") and os.environ.get("DOTFILES_TEST_GOAL_PLUGIN"), "set the Nix OpenCode and Goal plugin paths")
class GoalTests(unittest.TestCase):
    def test_goal_command_continues_and_persists_completion(self):
        calls = []
        objective = "Complete the local goal integration fixture"

        class ModelHandler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if not body.get("tools"):
                    delta = {"role": "assistant", "content": "Goal fixture"}
                    reason = "stop"
                else:
                    calls.append(body)
                    step = len(calls)
                    if step in (1, 3):
                        name = "create_goal" if step == 1 else "update_goal"
                        args = {"objective": objective} if step == 1 else {"status": "complete", "evidence": "The local fixture received automatic continuation."}
                        delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": f"goal_{step}", "type": "function", "function": {"name": name, "arguments": json.dumps(args)}}]}
                        reason = "tool_calls"
                    else:
                        delta = {"role": "assistant", "content": "Fixture progress has been recorded."}
                        reason = "stop"
                events = [
                    {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "fixture", "choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                    {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "fixture", "choices": [{"index": 0, "delta": {}, "finish_reason": reason}], "usage": {"prompt_tokens": 20000, "completion_tokens": 100, "total_tokens": 20100}},
                ]
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                for event in events:
                    self.wfile.write(f"data: {json.dumps(event)}\n\n".encode())
                self.wfile.write(b"data: [DONE]\n\n")

            def log_message(self, *args):
                pass

        with http.server.ThreadingHTTPServer(("127.0.0.1", 0), ModelHandler) as model, tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            worker = threading.Thread(target=model.serve_forever, daemon=True)
            worker.start()
            env = launcher.client_environment(root / "isolated")
            expected = launcher.profile("fixture", model.server_address[1], "fixture-token", os.environ["DOTFILES_TEST_GOAL_PLUGIN"])
            env["OPENCODE_CONFIG_CONTENT"] = json.dumps(expected)
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            command = [os.environ["DOTFILES_TEST_OPENCODE"], "serve", "--hostname", "127.0.0.1", "--port", str(port)]
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

            def request(path, body=None):
                data = json.dumps(body).encode() if body is not None else None
                req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data, headers={"Content-Type": "application/json"})
                with opener.open(req, timeout=20) as response:
                    return json.load(response)

            def wait_ready(process):
                for _ in range(100):
                    if process.poll() is not None:
                        self.fail("OpenCode server exited")
                    try:
                        return request("/global/health")
                    except OSError:
                        time.sleep(0.1)
                self.fail("OpenCode server did not start")

            try:
                with launcher.owned_process(command, env=env, cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
                    wait_ready(process)
                    commands = request("/command")
                    names = {entry["name"] for entry in commands}
                    self.assertTrue({"goal", "pause_goal", "resume_goal"}.issubset(names), names)
                    provider = request("/provider")
                    local = next(item for item in provider["all"] if item["id"] == "localllm")
                    self.assertEqual(local["models"]["model"]["limit"]["context"], 262144)
                    session = request("/session", {})["id"]
                    request(f"/session/{session}/command", {"command": "goal", "arguments": objective, "model": "localllm/model"})
                    state_file = Path(env["XDG_DATA_HOME"]) / "opencode-goal-plugin/goals.json"
                    goal = None
                    for _ in range(150):
                        if state_file.exists():
                            goal = json.loads(state_file.read_text())["goals"].get(session)
                            if goal and goal["status"] == "complete" and len(calls) >= 4:
                                break
                        time.sleep(0.1)
                    self.assertIsNotNone(goal)
                    self.assertEqual(goal["status"], "complete", goal)
                    self.assertGreaterEqual(len(calls), 4)
                    self.assertEqual(goal["objective"], objective)
                    self.assertTrue(all(call["model"] == "fixture" for call in calls))
                with launcher.owned_process(command, env=env, cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
                    wait_ready(process)
                    request("/command")
                    self.assertEqual(json.loads(state_file.read_text())["goals"][session]["status"], "complete")
            finally:
                model.shutdown()
                worker.join()


if __name__ == "__main__":
    unittest.main()
