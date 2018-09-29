#!/bin/bash
DOTFILES=( .bash_profile .bashrc .vimrc .zshenv .zsh mybin )

for f in ${DOTFILES[@]}
do
  if [ -h $HOME/$f ]; then
    # ln -sv $HOME/dotfiles/$f $HOME/
    rm -rv $HOME/$f
  else
    # ln -sv $HOME/dotfiles/$f $HOME/$f
    echo "${f} is not symbolic link"
  fi
done
