#!/bin/bash

current_dir=$(cd $(dirname $0); pwd)
# deploy
if [ $current_dir = "$HOME/dotfiles/etc" ]; then
  cd ~/dotfiles
  for f in .??*
  do
    [[ "$f" == ".git" ]] && continue
    [[ "$f" == ".gitignore" ]] && continue
    [[ "$f" == ".DS_Store" ]] && continue
    [[ "$f" == ".vim" ]] && continue
    if [ -d $f ]; then
      ln -sfv ~/dotfiles/$f ~/
    else
      ln -sfv ~/dotfiles/$f ~/$f
    fi
    echo $f
  done
  ln -sfv ~/dotfiles/mybin ~/
  echo -e "\033[0;32mdotfiles are successfully deployed!\033[0m"
else
  echo "move to dotfiles"
  # echo $current_dir
fi
