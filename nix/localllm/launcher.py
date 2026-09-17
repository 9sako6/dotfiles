import argparse
import contextlib
import fcntl
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def metal_check():
    import mlx.core as mx

    if not mx.metal.is_available():
        raise RuntimeError("Metal is unavailable")
    with mx.stream(mx.gpu):
        result = mx.ones((32, 32)) @ mx.ones((32, 32))
        mx.eval(result)
    if result[0, 0].item() != 32:
        raise RuntimeError("Metal computation failed")
    print("Metal computation passed", flush=True)


def clean_environment(state):
    environment = {
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "HOME": str(state / "home"),
        "OPENCODE_TEST_HOME": str(state / "home"),
        "XDG_CACHE_HOME": str(state / "cache"),
        "XDG_CONFIG_HOME": str(state / "config"),
        "XDG_DATA_HOME": str(state / "data"),
        "XDG_STATE_HOME": str(state / "state"),
        "HF_HUB_OFFLINE": "1",
        "HF_HUB_DISABLE_TELEMETRY": "1",
        "TRANSFORMERS_OFFLINE": "1",
        "DO_NOT_TRACK": "1",
        "OPENCODE_PURE": "1",
        "OPENCODE_DISABLE_DEFAULT_PLUGINS": "1",
        "OPENCODE_DISABLE_EXTERNAL_SKILLS": "1",
        "OPENCODE_DISABLE_CLAUDE_CODE": "1",
        "OPENCODE_DISABLE_AUTOUPDATE": "1",
        "OPENCODE_DISABLE_MODELS_FETCH": "1",
        "OPENCODE_DISABLE_PROJECT_CONFIG": "1",
    }
    for key in ("TERM", "LANG", "TMPDIR"):
        if key in os.environ:
            environment[key] = os.environ[key]
    for name in ("home", "cache", "config", "data", "state"):
        (state / name).mkdir(parents=True, exist_ok=True)
    return environment


def client_environment(state):
    environment = {key: value for key, value in os.environ.items() if not key.startswith("OPENCODE_")}
    environment.update(clean_environment(state))
    environment.pop("OPENCODE_PURE")
    environment["HOME"] = str(Path.home())
    environment["PATH"] = os.environ.get("PATH", os.defpath)
    environment["OPENCODE_ENABLE_EXA"] = "1"
    return environment


def server_sandbox():
    profile = '(version 1) (allow default) (deny network*) (allow network-inbound (local ip "localhost:*"))'
    return ["/usr/bin/sandbox-exec", "-p", profile]


def request(port, token, path="/v1/models"):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", headers={"Authorization": f"Bearer {token}"})
    with opener.open(req, timeout=2) as response:
        return json.load(response)


def wait_ready(process, port, token, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("local server exited before becoming ready")
        try:
            models = request(port, token)
            if models.get("data"):
                return
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.25)
    raise RuntimeError("local server startup timed out")


@contextlib.contextmanager
def owned_process(command, **kwargs):
    process = subprocess.Popen(command, start_new_session=True, **kwargs)
    try:
        yield process
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


def profile(model, port, token, goal_plugin=None):
    return {
        "model": "localllm/model",
        "small_model": "localllm/model",
        "enabled_providers": ["localllm"],
        "share": "disabled",
        "autoupdate": False,
        "plugin": [Path(goal_plugin).as_uri()] if goal_plugin else [],
        "mcp": {},
        "instructions": [str(Path(__file__).with_name("progress-instructions.md"))],
        "skills": {"paths": [], "urls": []},
        "lsp": False,
        "formatter": False,
        "permission": {"webfetch": "allow", "websearch": "allow", "skill": "deny"},
        "provider": {
            "localllm": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Local MLX",
                "options": {"baseURL": f"http://127.0.0.1:{port}/v1", "apiKey": token},
                "models": {"model": {"id": model, "name": "Local model", "limit": {"context": 262144, "output": 1024}}},
            }
        },
    }


@contextlib.contextmanager
def server_process(command, environment, state, operation):
    with contextlib.ExitStack() as resources:
        output = {}
        if operation == "chat":
            log_path = state / "server.log"
            log = resources.enter_context(os.fdopen(os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w"))
            output = {"stdin": subprocess.DEVNULL, "stdout": log, "stderr": subprocess.STDOUT}
            print(f"Server log: {log_path}", flush=True)
        with owned_process(command, env=environment, **output) as server:
            yield server


def verify_profile(actual, expected):
    for key in ("model", "small_model", "enabled_providers", "share", "plugin", "mcp", "instructions", "skills", "lsp", "formatter"):
        if actual.get(key) != expected[key]:
            raise RuntimeError(f"OpenCode merged configuration conflicts with local profile: {key}")
    if set(actual.get("provider", {})) != {"localllm"}:
        raise RuntimeError("OpenCode loaded an unexpected provider")
    if actual.get("provider") != expected["provider"]:
        raise RuntimeError("OpenCode local provider configuration changed")


def run_client(settings, state, project, model, port, token, arguments):
    try:
        request(port, token)
    except (OSError, urllib.error.URLError) as error:
        raise RuntimeError("local server is unavailable; no fallback") from error
    expected = profile(model, port, token, settings.get("goal_plugin"))
    with tempfile.TemporaryDirectory(prefix="profile-", dir=state) as temporary:
        session = Path(temporary)
        environment = client_environment(session)
        environment["XDG_DATA_HOME"] = str(state / "data")
        environment["XDG_STATE_HOME"] = str(state / "state")
        environment["OPENCODE_CONFIG_CONTENT"] = json.dumps(expected)
        tui_config = Path(environment["XDG_CONFIG_HOME"]) / "opencode" / "tui.json"
        tui_config.parent.mkdir(parents=True, exist_ok=True)
        tui_config.write_text(json.dumps({"plugin": expected["plugin"]}))
        command = [settings["opencode"]]
        inspected = subprocess.run(command + ["debug", "config"], env=environment, cwd=project, capture_output=True, text=True, timeout=30)
        if inspected.returncode:
            status = f"exit code {inspected.returncode}"
            if inspected.returncode < 0:
                status = signal.Signals(-inspected.returncode).name
            raise RuntimeError(f"OpenCode merged configuration inspection failed ({status})")
        verify_profile(json.loads(inspected.stdout), expected)
        with owned_process(command + arguments, env=environment, cwd=project) as client:
            return client.wait()


def install_signals():
    def interrupted(signum, frame):
        raise KeyboardInterrupt
    for signum in (signal.SIGHUP, signal.SIGTERM):
        signal.signal(signum, interrupted)


def main():
    install_signals()
    parser = argparse.ArgumentParser()
    parser.add_argument("--settings", required=True)
    parser.add_argument("operation", choices=("check", "chat", "serve"))
    parser.add_argument("--project", default=os.getcwd())
    args, arguments = parser.parse_known_args()
    settings = json.loads(Path(args.settings).read_text())
    metal_check()
    if args.operation == "check":
        return 0
    data = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    if not data.is_absolute():
        data = Path.home() / ".local/share"
    state = data / "dotfiles/localllm"
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (state / "server.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError("a local model is already running") from error
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        token = secrets.token_urlsafe(32)
        with tempfile.TemporaryDirectory(prefix="server-", dir=state) as temporary:
            environment = clean_environment(Path(temporary))
            environment["MLX_VLM_SERVER_API_KEY"] = token
            command = server_sandbox() + [settings["python"], "-m", "mlx_vlm.server", "--host", "127.0.0.1", "--port", str(port), "--model", settings["model"], "--model-discovery", "served", "--max-tokens", "1024", "--prefill-step-size", "64", "--kv-bits", "4", "--quantized-kv-start", "0", "--max-num-seqs", "1"]
            with server_process(command, environment, state, args.operation) as server:
                wait_ready(server, port, token)
                print(f"Local model ready on 127.0.0.1:{port}", flush=True)
                if args.operation == "serve":
                    return server.wait()
                arguments = arguments[1:] if arguments[:1] == ["--"] else arguments
                return run_client(settings, state, args.project, settings["model"], port, token, arguments)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except (RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"localllm: {error}", file=sys.stderr)
        sys.exit(1)
