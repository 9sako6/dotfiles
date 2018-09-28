#!/bin/bash

current_dir=$(cd $(dirname $0); pwd)
# clean
if [ $current_dir = "$HOME/dotfiles/etc" ]; then
  cd ~/dotfiles
  for f in .??*
  do
    [[ "$f" == ".git" ]] && continue
    [[ "$f" == ".gitignore" ]] && continue
    [[ "$f" == ".DS_Store" ]] && continue
    [[ "$f" == ".vim" ]] && continue
    if [ -d $f ]; then
      rm -r ~/$f
    else
      rm ~/$f
    fi
  done
  rm -r ~/mybin
  echo -e "\033[0;32mdotfiles are successfully cleaned!\033[0m"
else
  echo "move to dotfiles"
  # echo $current_dir
fi
