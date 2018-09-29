#!/bin/bash
DOTFILES=( .bash_profile .bashrc .vimrc .zshenv .zsh mybin )

for f in ${DOTFILES[@]}
do
  if [ -d $f ]; then
    ln -sv $HOME/dotfiles/$f $HOME/
  else
    ln -sv $HOME/dotfiles/$f $HOME/$f
  fi
done
