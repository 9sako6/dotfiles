from pathlib import Path
import shutil

root = Path(__file__).parent
shutil.copyfile(root / 'ssh-agent.zsh', 'home/.zsh.d/ssh-agent.zsh')
shutil.copyfile(root / 'test_ssh_agent.py', 'nix/tests/test_ssh_agent.py')
