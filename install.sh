#!/bin/sh

if ! $git_exists || ! $vim_exists || ! $zsh_exists; then
  echo "\033[0;31mError: git or vim or zsh is not installed.\033[0m"
  echo "\033[0;31mPlease install them\033[0m"
  exit
fi

# zsh
if [ ! -d ~/.zsh ]; then
  mkdir ~/.zsh
fi
ln -sf ~/dotfiles/.zsh/.zshrc ~/.zsh/.zshrc
ln -sf ~/dotfiles/.zsh/.zshenv ~/.zshenv

# bash
ln -sf ~/dotfiles/.bash/.bash_profile ~/.bash_profile
ln -sf ~/dotfiles/.bash/.bashrc ~/.bashrc

# vim
ln -sf ~/dotfiles/.vimrc ~/.vimrc

echo "\033[0;32mdotfiles are successfully installed!\033[0m"
