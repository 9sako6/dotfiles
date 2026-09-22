import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path.cwd()
WORK = Path(os.environ['RUNNER_TEMP']) / 'minus-stack'
RESULTS = Path(os.environ['RUNNER_TEMP']) / 'stack-results.json'
PARENT = 'fix/minus-137-privacy-paths'
COAUTHOR = 'Co-authored-by: ChatGPT (GPT-6 Astra Pro) <noreply@openai.com>'
ENTRIES = [
    (138, 'fix/minus-138-ssh-recovery', 'fix(ssh): recover only unreachable owned agent sockets', ['nix/tests/test_ssh_agent.py']),
    (139, 'fix/minus-139-cli-contracts', 'docs: give CLI behavior a single consistent contract', []),
]


def run(*args, cwd=WORK):
    print('+', *args, flush=True)
    return subprocess.run(args, cwd=cwd, check=True)


def output(*args, cwd=WORK):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


# The immediately preceding reviewed branch is the only acceptable parent.
deadline = time.monotonic() + 600
while True:
    found = output('git', 'ls-remote', '--heads', 'origin', f'refs/heads/{PARENT}', cwd=ROOT)
    if found:
        base = found.split()[0]
        break
    if time.monotonic() >= deadline:
        raise RuntimeError('the validated parent branch is not available; no PR branch was created')
    time.sleep(10)
run('git', 'fetch', '--no-tags', 'origin', f'refs/heads/{PARENT}', cwd=ROOT)
if output('git', 'rev-parse', 'FETCH_HEAD', cwd=ROOT) != base:
    raise RuntimeError('parent changed while fetching; rerun against the new verified parent')
run('git', 'worktree', 'add', '--detach', str(WORK), base, cwd=ROOT)
results = []
RESULTS.write_text('[]\n')
for issue, branch, title, added in ENTRIES:
    if output('git', 'ls-remote', '--heads', 'origin', branch):
        raise RuntimeError(f'refusing to overwrite {branch}')
    run('python3', str(ROOT / f'.review/{issue}.py'))
    run('git', 'add', '-u')
    if added:
        run('git', 'add', '--', *added)
    changed = output('git', 'diff', '--cached', '--name-only').splitlines()
    allowed = {'home/.zsh.d/ssh-agent.zsh', 'nix/tests/test_ssh_agent.py'} if issue == 138 else {'cli/README.md', 'docs/operations.md', 'docs/repo-map.md'}
    if set(changed) != allowed:
        raise RuntimeError('unexpected review paths: ' + repr(changed))
    print('REVIEW-FILES', changed, flush=True)
    run('cargo', 'fmt', '--check', '--manifest-path', 'cli/Cargo.toml')
    run('cargo', 'clippy', '--locked', '--manifest-path', 'cli/Cargo.toml', '--all-targets', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--manifest-path', 'cli/Cargo.toml')
    run('cargo', 'build', '--locked', '--manifest-path', 'cli/Cargo.toml')
    run('bun', 'install', '--frozen-lockfile')
    run('bun', 'run', 'tsc', '--noEmit')
    run('bun', 'test', './tests', './home/.apm/skills/anki/tools/')
    if issue == 138:
        run('python3', '-m', 'unittest', 'discover', '-s', 'nix/tests', '-p', 'test_ssh_agent.py', '-v')
    else:
        run('nix', 'build', '--no-link', '.#checks.aarch64-darwin.composition', '.#checks.aarch64-darwin.configuration')
        run('python3', '-m', 'unittest', 'discover', '-s', 'nix/tests', '-p', 'test_*.py', '-v')
        opencode = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmClient')
        goal = output('nix', 'build', '--no-link', '--print-out-paths', '.#localllmGoalPlugin')
        os.environ['DOTFILES_TEST_OPENCODE'] = opencode + '/bin/opencode'
        os.environ['DOTFILES_TEST_GOAL_PLUGIN'] = goal
        run('python3', '-m', 'unittest', 'discover', '-s', 'nix/localllm', '-p', 'test_*.py', '-v')
    run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
        '-c', 'core.hooksPath=/dev/null', 'commit', '-m', title, '-m', f'Fixes #{issue}', '-m', COAUTHOR)
    sha = output('git', 'rev-parse', 'HEAD')
    run('git', 'push', f'--force-with-lease=refs/heads/{branch}:', 'origin', f'HEAD:refs/heads/{branch}')
    results.append({'issue': issue, 'branch': branch, 'commit': sha})
    RESULTS.write_text(json.dumps(results, indent=2))
    print('STACK-COMMIT', issue, branch, sha, flush=True)
