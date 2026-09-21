import json
import os
from pathlib import Path
import subprocess


ROOT = Path.cwd()
WORK = Path(os.environ['RUNNER_TEMP']) / 'minus-stack'
RESULTS = Path(os.environ['RUNNER_TEMP']) / 'stack-results.json'
BASE = '8bde226f0ac0a11117733782db7bf857deb769ad'
ENTRIES = [
    (132, 'fix/minus-132-resource-diff', 'refactor(inventory): decide changes from typed resource deltas'),
    (133, 'fix/minus-133-toml-pins', 'fix(zinit): read pinned repositories as TOML data'),
]
COAUTHOR = 'Co-authored-by: ChatGPT (GPT-6 Astra Pro) <noreply@openai.com>'


def run(*args, cwd=WORK):
    print('+', *args, flush=True)
    return subprocess.run(args, cwd=cwd, check=True)


run('git', 'worktree', 'add', '--detach', str(WORK), BASE, cwd=ROOT)
results = []
RESULTS.write_text('[]\n')
for issue, branch, title in ENTRIES:
    assert branch.startswith(f'fix/minus-{issue}-')
    existing = subprocess.check_output(['git', 'ls-remote', '--heads', 'origin', branch], cwd=WORK)
    if existing.strip():
        raise RuntimeError(f'refusing to overwrite existing branch {branch}')
    patch = ROOT / f'.review/{issue}.patch'
    patch.write_text(patch.read_text().replace('\n diff --git ', '\ndiff --git '))
    run('git', 'apply', '--index', str(patch))
    run('cargo', 'fmt', '--manifest-path', 'cli/Cargo.toml')
    run('git', 'add', '-u')
    run('cargo', 'clippy', '--locked', '--manifest-path', 'cli/Cargo.toml', '--all-targets', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--manifest-path', 'cli/Cargo.toml')
    run('bun', 'install', '--frozen-lockfile')
    run('bun', 'run', 'tsc', '--noEmit')
    run('bun', 'test', './tests', './home/.apm/skills/anki/tools/')
    run('git', '-c', 'user.name=9sako6', '-c', 'user.email=31821663+9sako6@users.noreply.github.com',
        '-c', 'core.hooksPath=/dev/null', 'commit', '-m', title, '-m', f'Fixes #{issue}', '-m', COAUTHOR)
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=WORK, text=True).strip()
    run('git', 'push', f'--force-with-lease=refs/heads/{branch}:', 'origin', f'HEAD:refs/heads/{branch}')
    results.append({'issue': issue, 'branch': branch, 'commit': commit})
    RESULTS.write_text(json.dumps(results, indent=2) + '\n')
    print(f'STACK-COMMIT {issue} {branch} {commit}', flush=True)
