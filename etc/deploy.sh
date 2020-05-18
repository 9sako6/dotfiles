#!/bin/bash
export DOTFILES_PATH="${HOME}/dotfiles"
dotfiles=(.bash_profile .bashrc .vimrc .zshenv .zshrc mybin .commit_template .tmux.conf .xonshrc .gitignore_global)

for f in ${dotfiles[@]}; do
  if [ -d ${f} ] && [ ! -e "${HOME}/${f}" ]; then
    ln -sv "${DOTFILES_PATH}/${f}" "${HOME}"
  else
    if [ ! -e "${HOME}/${f}" ]; then
      ln -sv "${DOTFILES_PATH}/${f}" "${HOME}/${f}"
    fi
  fi
done

# git
bash "${DOTFILES_PATH}/etc/git/init.sh"

# nvim
bash "${DOTFILES_PATH}/etc/nvim/init.sh"

# vscode
if type code >/dev/null 2>&1; then
  bash "${DOTFILES_PATH}/etc/vscode/init.sh"
fi
