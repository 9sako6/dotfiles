#!/bin/bash

echo '     _       _    __ _ _            '
echo '    | |     | |  / _(_) |           '
echo '  __| | ___ | |_| |_ _| | ___  ___  '
echo ' / _` |/ _ \| __|  _| | |/ _ \/ __| '
echo '| (_| | (_) | |_| | | | |  __/\__ \ '
echo ' \__,_|\___/ \__|_| |_|_|\___||___/ '
echo ''

if ! $git_exists || ! $vim_exists || ! $zsh_exists; then
  echo -e "\033[0;31mError: git or vim or zsh is not installed.\033[0m"
  echo -e "\033[0;31mPlease install them\033[0m"
  exit
fi

if [ ! -d ~/dotfiles ]; then
  cd ~
  git clone https://github.com/9sako6/dotfiles
fi

# zplug
if [ ! -d ~/.zplug ]; then
  mkdir -p ~/.zplug
  curl -sL --proto-redir -all,https https://raw.githubusercontent.com/zplug/installer/master/installer.zsh| zsh
fi

# vim-plug
if [ ! -e ~/.vim/autoload/plug.vim ]; then
  curl -fLo ~/.vim/autoload/plug.vim --create-dirs \
      https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim
fi



echo -e "\033[0;32mdotfiles are successfully initialized!\033[0m"
echo -e "To finish vim settings, do \033[0;32m:PlugInstall\033[0m in vim console"
