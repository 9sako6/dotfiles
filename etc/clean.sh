#!/bin/bash
DOTFILES=(.bash_profile .bashrc .vimrc .zshenv .zshrc .zshenv.zwc .zshrc.zwc mybin .commit_template .tmux.conf .xonshrc .zshrc.zwc .zshenv.zwc .zsh_history .zcompdump .zplugin .gitignore_global)

for f in ${DOTFILES[@]}; do
  if [ -d "${HOME}"/"${f}" ]; then
    echo "${f}"
    rm -rfv "${HOME}"/"${f}"
  else
    echo "${f}"
    rm -fv "${HOME}"/"${f}"
  fi
done
