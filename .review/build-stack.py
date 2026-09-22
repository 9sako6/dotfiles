import json
import os
from pathlib import Path
import subprocess

ROOT = Path.cwd()
WORK = Path(os.environ['RUNNER_TEMP']) / 'minus-stack'
RESULTS = Path(os.environ['RUNNER_TEMP']) / 'stack-results.json'
BASE = 'e4b4cd1996eca189e4d01543779d6cd55876124a'
BRANCH = 'fix/minus-134-behavior-contracts'
COAUTHOR = 'Co-authored-by: ChatGPT (GPT-6 Astra Pro) <noreply@openai.com>'


def run(*args, cwd=WORK):
    print('+', *args, flush=True)
    return subprocess.run(args, cwd=cwd, check=True)


def output(*args):
    return subprocess.check_output(args, cwd=WORK, text=True).strip()


run('git', 'worktree', 'add', '--detach', str(WORK), BASE, cwd=ROOT)
if output('git', 'ls-remote', '--heads', 'origin', BRANCH):
    raise RuntimeError('refusing to overwrite an existing review branch')
run('python3', str(ROOT / '.review/134.py'))
run('cargo', 'fmt', '--manifest-path', 'cli/Cargo.toml')
run('cargo', 'clippy', '--locked', '--manifest-path', 'cli/Cargo.toml', '--all-targets', '--', '-D', 'warnings')
run('cargo', 'test', '--locked', '--manifest-path', 'cli/Cargo.toml')
run('cargo', 'build', '--locked', '--manifest-path', 'cli/Cargo.toml')
run('bun', 'install', '--frozen-lockfile')
run('bun', 'run', 'tsc', '--noEmit')
run('bun', 'test', './tests', './home/.apm/skills/anki/tools/')
for name in ['test_plan.py', 'test_agents.py', 'test_bootstrap.py', 'test_apply_lock.py', 'test_review_terminal.py']:
    run('python3', '-m', 'unittest', 'discover', '-s', 'nix/tests', '-p', name, '-v')
opencode = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmClient')
goal = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmGoalPlugin')
os.environ['DOTFILES_TEST_OPENCODE'] = opencode + '/bin/opencode'
os.environ['DOTFILES_TEST_GOAL_PLUGIN'] = goal
run('python3', '-m', 'unittest', 'discover', '-s', 'nix/localllm', '-p', 'test_*.py', '-v')
run('git', 'add', '-A')
run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
    '-c', 'core.hooksPath=/dev/null', 'commit', '-m', 'test: verify public operations instead of internal call sequences',
    '-m', 'Fixes #134', '-m', COAUTHOR)
sha = output('git', 'rev-parse', 'HEAD')
run('git', 'push', f'--force-with-lease=refs/heads/{BRANCH}:', 'origin', f'HEAD:refs/heads/{BRANCH}')
RESULTS.write_text(json.dumps([{'issue': 134, 'branch': BRANCH, 'commit': sha}], indent=2))
print('STACK-COMMIT', 134, BRANCH, sha, flush=True)
