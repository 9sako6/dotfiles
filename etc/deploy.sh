#!/bin/bash
DOTFILES=( .bash_profile .bashrc .vimrc .zshenv .zshrc mybin .commit_template .tmux.conf .xonshrc)

for f in ${DOTFILES[@]}
do
  if [ -d ${f} ]; then
    ln -sv "${HOME}/dotfiles/${f} ${HOME}"
  else
    ln -sv "${HOME}/dotfiles/${f} ${HOME}/${f}"
  fi
done

ln -sv "${HOME}/dotfiles/.vscode/snippets" "${HOME}/Library/Application Support/Code/User/"