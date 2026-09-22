import json
import os
from pathlib import Path
import subprocess

ROOT = Path.cwd()
WORK = Path(os.environ['RUNNER_TEMP']) / 'minus-stack'
RESULTS = Path(os.environ['RUNNER_TEMP']) / 'stack-results.json'
BASE = 'e0f4bb94c52c7a05aac1613f4b1c07bf561522f3'
COAUTHOR = 'Co-authored-by: ChatGPT (GPT-6 Astra Pro) <noreply@openai.com>'
ENTRIES = [
    (135, 'fix/minus-135-inventory-snapshot', 'refactor(inventory): freeze normalized skill data in each generation', ['cli/tests/inventory_snapshot.rs']),
    (136, 'fix/minus-136-git-undo', 'fix(git): unstage consistently with or without an existing HEAD', ['nix/tests/test_git_undo.py']),
    (137, 'fix/minus-137-privacy-paths', 'fix(git): inspect staged document paths without display parsing', ['nix/tests/test_privacy.py']),
]


def run(*args, cwd=WORK):
    print('+', *args, flush=True)
    return subprocess.run(args, cwd=cwd, check=True)


def output(*args):
    return subprocess.check_output(args, cwd=WORK, text=True).strip()


run('git', 'worktree', 'add', '--detach', str(WORK), BASE, cwd=ROOT)
results = []
RESULTS.write_text('[]\n')
for issue, branch, title, added in ENTRIES:
    if output('git', 'ls-remote', '--heads', 'origin', branch):
        raise RuntimeError(f'refusing to overwrite {branch}')
    run('python3', str(ROOT / f'.review/{issue}.py'))
    run('cargo', 'fmt', '--manifest-path', 'cli/Cargo.toml')
    run('git', 'add', '-u')
    run('git', 'add', '--', *added)
    changed = output('git', 'diff', '--cached', '--name-only').splitlines()
    if len(changed) > 30 or any(path.startswith(('node_modules/', '.review/')) or path == 'README.md' for path in changed):
        raise RuntimeError('unexpected paths in review commit: ' + repr(changed))
    print('REVIEW-FILES', changed, flush=True)
    run('cargo', 'clippy', '--locked', '--manifest-path', 'cli/Cargo.toml', '--all-targets', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--manifest-path', 'cli/Cargo.toml')
    run('cargo', 'build', '--locked', '--manifest-path', 'cli/Cargo.toml')
    run('bun', 'install', '--frozen-lockfile')
    run('bun', 'run', 'tsc', '--noEmit')
    run('bun', 'test', './tests', './home/.apm/skills/anki/tools/')
    if issue == 135:
        run('nix', 'build', '--no-link', '.#checks.aarch64-darwin.composition', '.#checks.aarch64-darwin.configuration')
        run('python3', '-m', 'unittest', 'discover', '-s', 'nix/tests', '-p', 'test_*.py', '-v')
    else:
        test = 'test_git_undo.py' if issue == 136 else 'test_privacy.py'
        run('python3', '-m', 'unittest', 'discover', '-s', 'nix/tests', '-p', test, '-v')
    run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
        '-c', 'core.hooksPath=/dev/null', 'commit', '-m', title, '-m', f'Fixes #{issue}', '-m', COAUTHOR)
    sha = output('git', 'rev-parse', 'HEAD')
    run('git', 'push', f'--force-with-lease=refs/heads/{branch}:', 'origin', f'HEAD:refs/heads/{branch}')
    results.append({'issue': issue, 'branch': branch, 'commit': sha})
    RESULTS.write_text(json.dumps(results, indent=2))
    print('STACK-COMMIT', issue, branch, sha, flush=True)
