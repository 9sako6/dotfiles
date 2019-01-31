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

if [ ! -d "$HOME"/dotfiles ]; then
  cd "$HOME"
  git clone https://github.com/9sako6/dotfiles
fi

# zplugin
if [ ! -d "$HOME"/.zplugin ]; then
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/zdharma/zplugin/master/doc/install.sh)"
fi

# vim-plug
if [ ! -e "$HOME"/.vim/autoload/plug.vim ]; then
  curl -fLo "$HOME"/.vim/autoload/plug.vim --create-dirs \
      https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim
fi

# fzf
if [ ! -d "$HOME"/.fzf ]; then
  mkdir -p "$HOME"/.fzf
  git clone --depth 1 https://github.com/junegunn/fzf.git "$HOME"/.fzf
  "$HOME"/.fzf/install
fi

echo -e "\033[0;32mdotfiles are successfully initialized!\033[0m"
echo -e "To finish vim settings, do \033[0;32m:PlugInstall\033[0m in vim console"
