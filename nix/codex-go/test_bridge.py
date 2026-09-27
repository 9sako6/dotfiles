import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


PACKAGE = os.environ.get("DOTFILES_TEST_CODEX_GO")


class Upstream(BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.requests.append((self.path, dict(self.headers), body))
        completed = any(message.get("role") == "tool" for message in body["messages"])
        message = {"role": "assistant", "content": "verified"}
        reason = "stop"
        if not completed:
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [{
                    "id": "call_fixture",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": '{"path":"fixture.txt"}'},
                }],
            }
            reason = "tool_calls"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        for call in message.get("tool_calls", []):
            call["index"] = 0
        chunk = {
            "id": "chatcmpl-fixture",
            "object": "chat.completion.chunk",
            "created": 1700000000,
            "model": body["model"],
            "choices": [{"index": 0, "delta": message, "finish_reason": None}],
        }
        self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
        chunk["choices"] = [{"index": 0, "delta": {}, "finish_reason": reason}]
        chunk["usage"] = {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
        self.wfile.write(("data: " + json.dumps(chunk) + "\n\ndata: [DONE]\n\n").encode())

    def log_message(self, *args):
        pass


@unittest.skipUnless(PACKAGE, "Set DOTFILES_TEST_CODEX_GO to the built codexGo package")
class BridgeTest(unittest.TestCase):
    def test_authenticated_streaming_tools_and_session_headers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "opencode").mkdir()
            (root / "opencode/auth.json").write_text(json.dumps({
                "opencode-go": {"type": "api", "key": "fixture-token"},
            }))
            environment = {**os.environ, "XDG_DATA_HOME": directory}
            auth = subprocess.run([f"{PACKAGE}/bin/codex-go-auth"], env=environment, capture_output=True, check=True)
            self.assertEqual(auth.stdout.strip(), b"fixture-token")
            (root / "opencode/auth.json").write_text("{}")
            missing = subprocess.run([f"{PACKAGE}/bin/codex-go-auth"], env=environment, capture_output=True)
            self.assertNotEqual(missing.returncode, 0)
            self.assertEqual(missing.stdout, b"")
            (root / "opencode/auth.json").write_text(json.dumps({
                "opencode-go": {"type": "api", "key": "fixture-token"},
            }))
            upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
            Upstream.requests = []
            thread = threading.Thread(target=upstream.serve_forever, daemon=True)
            thread.start()
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                port = listener.getsockname()[1]
            config = json.loads(Path(f"{PACKAGE}/share/codex-go/proxy.yaml").read_text())
            config["model_list"][0]["litellm_params"]["api_base"] = f"http://127.0.0.1:{upstream.server_port}/v1"
            config_path = root / "proxy.json"
            config_path.write_text(json.dumps(config))
            with (root / "proxy.log").open("wb") as log:
                process = subprocess.Popen([
                    f"{PACKAGE}/bin/codex-go-proxy", "--config", str(config_path), "--port", str(port),
                ], env=environment, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 45
                    while True:
                        try:
                            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                                break
                        except OSError:
                            self.assertIsNone(process.poll(), "Proxy exited before listening")
                            self.assertLess(time.monotonic(), deadline, "Proxy did not start")
                            time.sleep(0.1)
                    url = f"http://127.0.0.1:{port}/v1/responses"
                    headers = {
                        "Authorization": "Bearer fixture-token",
                        "Content-Type": "application/json",
                        "x-codex-turn-metadata": json.dumps({"session_id": "fixture-session"}),
                    }
                    tools = [{
                        "type": "function", "name": "read_file", "description": "Read a fixture",
                        "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
                    }]
                    inputs = [{"role": "user", "content": "Read fixture.txt and verify it"}]

                    def response(items, token="fixture-token"):
                        request = urllib.request.Request(url, data=json.dumps({
                            "model": "deepseek-v4.1-flash", "input": items, "tools": tools,
                            "stream": True, "reasoning": {"effort": "none"},
                        }).encode(), headers={**headers, "Authorization": f"Bearer {token}"})
                        with urllib.request.urlopen(request, timeout=30) as stream:
                            events = [json.loads(line[6:]) for line in stream if line.startswith(b"data: {")]
                        return next(event["response"] for event in events if event.get("type") == "response.completed")

                    with self.assertRaises(urllib.error.HTTPError) as error:
                        response(inputs, token="wrong-token")
                    self.assertTrue(400 <= error.exception.code < 500)
                    self.assertEqual(Upstream.requests, [])
                    first = response(inputs)
                    call = next(item for item in first["output"] if item["type"] == "function_call")
                    self.assertEqual(call["name"], "read_file")
                    self.assertEqual(json.loads(call["arguments"]), {"path": "fixture.txt"})
                    second = response(inputs + [call, {"type": "function_call_output", "call_id": call["call_id"], "output": "fixture content"}])
                    self.assertTrue(any(part.get("text") == "verified" for item in second["output"] for part in item.get("content", [])))
                    self.assertEqual(len(Upstream.requests), 2)
                    for path, forwarded_headers, body in Upstream.requests:
                        forwarded = {key.lower(): value for key, value in forwarded_headers.items()}
                        self.assertEqual(path, "/v1/chat/completions")
                        self.assertEqual(forwarded["user-agent"], "codex-go/1.0")
                        self.assertEqual(json.loads(forwarded["x-codex-turn-metadata"])["session_id"], "fixture-session")
                        self.assertEqual(body["model"], "deepseek-v4.1-flash")
                finally:
                    process.terminate()
                    process.wait(timeout=15)
                    upstream.shutdown()
                    upstream.server_close()
                    thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
