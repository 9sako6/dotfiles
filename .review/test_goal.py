"""Verify goal persistence through OpenCode tools, not the plugin's files."""
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


@unittest.skipUnless(os.environ.get('DOTFILES_TEST_OPENCODE') and os.environ.get('DOTFILES_TEST_GOAL_PLUGIN'),
                     'set the Nix OpenCode and Goal plugin paths')
class GoalTests(unittest.TestCase):
    def test_goal_command_continues_and_persists_completion(self):
        objective = 'Complete the local goal integration fixture'
        completed = threading.Event()
        phase = {'readback': False, 'steps': 0}
        readback = []
        models = []

        class ModelHandler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                models.append(body['model'])
                tool = None
                content = 'Goal fixture'
                if body.get('tools') and phase['readback']:
                    replies = [message for message in body['messages']
                               if message.get('role') == 'tool' and message.get('tool_call_id') == 'goal_readback']
                    if replies:
                        readback.extend(replies)
                        content = 'Readback complete'
                    else:
                        tool = ('goal_readback', 'get_goal', {})
                elif body.get('tools'):
                    phase['steps'] += 1
                    step = phase['steps']
                    if step == 1:
                        tool = ('goal_create', 'create_goal', {'objective': objective})
                    elif step == 3:
                        tool = ('goal_complete', 'update_goal', {
                            'status': 'complete', 'evidence': 'Received automatic continuation.'})
                    elif step >= 4:
                        completed.set()
                    content = 'Fixture progress has been recorded.'
                if tool:
                    identifier, name, arguments = tool
                    delta = {'role': 'assistant', 'tool_calls': [{
                        'index': 0, 'id': identifier, 'type': 'function',
                        'function': {'name': name, 'arguments': json.dumps(arguments)}}]}
                    reason = 'tool_calls'
                else:
                    delta = {'role': 'assistant', 'content': content}
                    reason = 'stop'
                events = [
                    {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': 0, 'model': 'fixture',
                     'choices': [{'index': 0, 'delta': delta, 'finish_reason': None}]},
                    {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': 0, 'model': 'fixture',
                     'choices': [{'index': 0, 'delta': {}, 'finish_reason': reason}],
                     'usage': {'prompt_tokens': 20000, 'completion_tokens': 100, 'total_tokens': 20100}},
                ]
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.end_headers()
                for event in events:
                    self.wfile.write(f'data: {json.dumps(event)}\n\n'.encode())
                self.wfile.write(b'data: [DONE]\n\n')

            def log_message(self, *args):
                pass

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), ModelHandler) as model, tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            worker = threading.Thread(target=model.serve_forever, daemon=True)
            worker.start()
            env = launcher.client_environment(root / 'isolated')
            expected = launcher.profile('fixture', model.server_address[1], 'fixture-token',
                                        os.environ['DOTFILES_TEST_GOAL_PLUGIN'])
            env['OPENCODE_CONFIG_CONTENT'] = json.dumps(expected)
            with socket.socket() as reservation:
                reservation.bind(('127.0.0.1', 0))
                port = reservation.getsockname()[1]
            command = [os.environ['DOTFILES_TEST_OPENCODE'], 'serve', '--hostname', '127.0.0.1', '--port', str(port)]
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

            def request(path, body=None):
                data = json.dumps(body).encode() if body is not None else None
                req = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data,
                                             headers={'Content-Type': 'application/json'})
                with opener.open(req, timeout=30) as response:
                    return json.load(response)

            def wait_ready(process):
                for _ in range(100):
                    if process.poll() is not None:
                        self.fail('OpenCode server exited')
                    try:
                        return request('/global/health')
                    except OSError:
                        time.sleep(0.1)
                self.fail('OpenCode server did not start')

            try:
                with launcher.owned_process(command, env=env, cwd=root,
                                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
                    wait_ready(process)
                    names = {entry['name'] for entry in request('/command')}
                    self.assertTrue({'goal', 'pause_goal', 'resume_goal'}.issubset(names), names)
                    session = request('/session', {})['id']
                    request(f'/session/{session}/command', {
                        'command': 'goal', 'arguments': objective, 'model': 'localllm/model'})
                    self.assertTrue(completed.wait(15), 'goal did not automatically continue to completion')
                phase['readback'] = True
                with launcher.owned_process(command, env=env, cwd=root,
                                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
                    wait_ready(process)
                    request('/command')
                    request(f'/session/{session}/message', {
                        'model': {'providerID': 'localllm', 'modelID': 'model'},
                        'parts': [{'type': 'text', 'text': 'Read the current goal using get_goal.'}],
                    })
                    self.assertTrue(readback, 'get_goal did not produce a tool result after restart')
                    observed = json.dumps(readback)
                    self.assertIn(objective, observed)
                    self.assertIn('complete', observed)
                self.assertTrue(models)
                self.assertTrue(all(name == 'fixture' for name in models))
            finally:
                model.shutdown()
                worker.join()


if __name__ == '__main__':
    unittest.main()
