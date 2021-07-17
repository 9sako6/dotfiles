#!/bin/bash
# set -e

curl -fLo ~/.vim/autoload/plug.vim --create-dirs \
  https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim
echo 'Please run `:PlugInstall` in vim'
