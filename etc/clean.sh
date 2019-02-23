#!/bin/bash
DOTFILES=( .bash_profile .bashrc .vimrc .zshenv .zshrc .zshenv.zwc .zshrc.zwc mybin .commit_template .tmux.conf .xonshrc)

for f in ${DOTFILES[@]}
do
  if [ -h "$HOME"/"$f" ]; then
    rm -rv "$HOME"/"$f"
  else
    echo "${f} is not symbolic link"
  fi
done
