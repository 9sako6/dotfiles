from pathlib import Path
import shutil

path = Path('home/mybin/git-undo')
text = path.read_text()
assert text.count('git restore --staged -- "$@"') == 1
assert text.count('git restore --staged .') == 1
text = text.replace('git restore --staged -- "$@"', 'git reset --quiet -- "$@"')
text = text.replace('git restore --staged .', 'git reset --quiet -- :/')
path.write_text(text)
shutil.copyfile(Path(__file__).with_name('test_git_undo.py'), 'nix/tests/test_git_undo.py')
