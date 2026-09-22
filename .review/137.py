from pathlib import Path
import shutil

root = Path(__file__).parent
shutil.copyfile(root / 'privacy-check', 'home/.config/git/hooks/check-public-document-privacy')
shutil.copyfile(root / 'test_privacy.py', 'nix/tests/test_privacy.py')
