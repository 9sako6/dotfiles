import json
import os
from pathlib import Path
import subprocess

ROOT = Path.cwd()
WORK = Path(os.environ['RUNNER_TEMP']) / 'minus-stack'
RESULTS = Path(os.environ['RUNNER_TEMP']) / 'stack-results.json'
COAUTHOR = 'Co-authored-by: ChatGPT (GPT-6 Astra Pro) <noreply@openai.com>'
ENTRIES = [
    ('fix/minus-130-process-tree', '111238beee6eac77c2229e01b807965def95ac6d'),
    ('fix/minus-131-apply-lock', '8bde226f0ac0a11117733782db7bf857deb769ad'),
    ('fix/minus-132-resource-diff', '90c719b7bfc144435dc9be8c784f3b9199113ffe'),
    ('fix/minus-133-toml-pins', 'a8d08e66781711bb062fc5b75e44e0893dccaf0d'),
]


def run(*args, cwd=WORK):
    print('+', *args, flush=True)
    return subprocess.run(args, cwd=cwd, check=True)


def output(*args):
    return subprocess.check_output(args, cwd=WORK, text=True).strip()


def commit(message):
    run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
        '-c', 'core.hooksPath=/dev/null', 'commit', '-m', message, '-m', COAUTHOR)


run('git', 'worktree', 'add', '--detach', str(WORK), ENTRIES[0][1], cwd=ROOT)
path = WORK / 'nix/localllm/launcher.py'
text = path.read_text()
start = text.index('def stop_process_group(process):')
end = text.index('\n\n@contextlib.contextmanager', start)
text = text[:start] + '''def process_group_running(group):
    # A process-table entry for an exited child is not a running process.
    # Darwin can return EPERM from killpg(group, 0) for an exiting group.
    result = subprocess.run(
        ["/bin/ps", "-axo", "pid=,pgid=,stat="],
        capture_output=True, text=True, check=True,
    )
    for line in result.stdout.splitlines():
        _, pgid, state = line.split()
        if int(pgid) == group and not state.startswith(("Z", "X")):
            return True
    return False


def signal_process_group(group, signum):
    try:
        os.killpg(group, signum)
    except (ProcessLookupError, PermissionError):
        # Do not swallow a real permission failure for a live process.
        if process_group_running(group):
            raise


def stop_process_group(process):
    """Stop all executing members; do not wait for orphan zombie reaping."""
    signal_process_group(process.pid, signal.SIGTERM)
    deadline = time.monotonic() + _PROCESS_STOP_TIMEOUT
    while process_group_running(process.pid):
        if time.monotonic() >= deadline:
            signal_process_group(process.pid, signal.SIGKILL)
            deadline = time.monotonic() + 5
            while process_group_running(process.pid):
                if time.monotonic() >= deadline:
                    raise RuntimeError("owned process group did not stop")
                time.sleep(0.05)
            break
        time.sleep(0.05)
    process.wait()
''' + text[end:]
path.write_text(text)
path = WORK / 'nix/localllm/test_process_tree.py'
text = path.read_text()
needle = 'class ProcessTreeTests(unittest.TestCase):\n'
assert text.count(needle) == 1
text = text.replace(needle, needle + '''    def test_fast_exit_and_normal_termination_are_reaped(self):
        for script in ("pass", "import time; time.sleep(60)"):
            for _ in range(8):
                with launcher.owned_process([sys.executable, "-c", script]) as process:
                    if script == "pass":
                        process.wait(timeout=5)
                self.assertIsNotNone(process.returncode)

''')
text = text.replace('with contextlib.suppress(ProcessLookupError):\n                            os.killpg(group, signal.SIGKILL)',
                    'launcher.signal_process_group(group, signal.SIGKILL)')
text = text.replace('with contextlib.suppress(ProcessLookupError):\n                        os.killpg(group, signal.SIGKILL)',
                    'launcher.signal_process_group(group, signal.SIGKILL)')
path.write_text(text)
run('python3', '-m', 'unittest', 'discover', '-s', 'nix/localllm', '-p', 'test_process_tree.py', '-v')
run('python3', '-m', 'unittest', 'discover', '-s', 'nix/localllm', '-p', 'test_launcher.py', '-v')
run('git', 'add', 'nix/localllm/launcher.py', 'nix/localllm/test_process_tree.py')
commit('fix(localllm): distinguish exiting groups from running descendants on macOS')
results = []
previous = None
for index, (branch, expected) in enumerate(ENTRIES):
    actual = output('git', 'ls-remote', '--heads', 'origin', branch).split()[0]
    if actual != expected:
        raise RuntimeError(f'branch moved, refusing update: {branch}')
    if index:
        run('git', 'checkout', '--detach', expected)
        run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
            '-c', 'core.hooksPath=/dev/null', 'merge', '--no-ff', previous,
            '-m', 'Merge the tested parent process-lifecycle fix\n\n' + COAUTHOR)
    sha = output('git', 'rev-parse', 'HEAD')
    run('git', 'push', 'origin', f'HEAD:refs/heads/{branch}')
    results.append({'branch': branch, 'commit': sha})
    RESULTS.write_text(json.dumps(results, indent=2))
    print('STACK-COMMIT', branch, sha, flush=True)
    previous = sha
opencode = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmClient')
goal = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmGoalPlugin')
os.environ['DOTFILES_TEST_OPENCODE'] = opencode + '/bin/opencode'
os.environ['DOTFILES_TEST_GOAL_PLUGIN'] = goal
run('python3', '-m', 'unittest', 'discover', '-s', 'nix/localllm', '-p', 'test_*.py', '-v')
print('PROCESS-REPAIR-VALIDATED', flush=True)
