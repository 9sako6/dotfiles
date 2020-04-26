#!/bin/bash
DOTFILES_PATH="${HOME}/dotfiles"
dotfiles=( .bash_profile .bashrc .vimrc .zshenv .zshrc mybin .commit_template .tmux.conf .xonshrc )

for f in ${dotfiles[@]}
do
  if [ -d ${f} ] && [ ! -e "${HOME}/${f}" ]; then
    ln -sv "${DOTFILES_PATH}/${f}" "${HOME}"
  else
    if [ ! -e "${HOME}/${f}" ]; then
      ln -sv "${DOTFILES_PATH}/${f}" "${HOME}/${f}"
    fi
  fi
done

# nvim
if [ ! -e "${HOME}/.config/nvim" ]; then
  if [ ! -e "${HOME}/.config" ]; then
    mkdir "${HOME}/.config"
  fi
  ln -sv "${DOTFILES_PATH}/.config/nvim" "${HOME}/.config"
fi

# vscode
bash "${DOTFILES_PATH}/etc/vscode/init.sh"
