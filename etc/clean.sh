#!/bin/bash
DOTFILES=( .bash_profile .bashrc .vimrc .zshenv .zsh mybin )

for f in ${DOTFILES[@]}
do
  if [ -h $HOME/$f ]; then
    rm -rv $HOME/$f
  else
    echo "${f} is not symbolic link"
  fi
done
