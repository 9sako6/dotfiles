#!/bin/bash
dotfiles=( .bash_profile .bashrc .vimrc .zshenv .zshrc mybin .commit_template .tmux.conf .xonshrc )

for f in ${dotfiles[@]}
do
  if [ -d ${f} ]; then
    ln -sv "${HOME}/dotfiles/${f}" "${HOME}"
  else
    ln -sv "${HOME}/dotfiles/${f}" "${HOME}/${f}"
  fi
done

# vscode
vscode_files=( snippets keybindings.json settings.json )

for f in ${vscode_files[@]}
do
  if [ -e "${HOME}/Library/Application Support/Code/User" ]; then
    ln -sv "${HOME}/dotfiles/.vscode/${f}" "${HOME}/Library/Application Support/Code/User"
  fi
done