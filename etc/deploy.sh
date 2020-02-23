#!/bin/bash
dotfiles=( .bash_profile .bashrc .vimrc .zshenv .zshrc mybin .commit_template .tmux.conf .xonshrc )

for f in ${dotfiles[@]}
do
  if [ -d ${f} ] && [ ! -e "${HOME}/${f}" ]; then
    ln -sv "${HOME}/dotfiles/${f}" "${HOME}"
  else
    if [ ! -e "${HOME}/${f}" ]; then
      ln -sv "${HOME}/dotfiles/${f}" "${HOME}/${f}"
    fi
  fi
done

# nvim
if [ ! -e "${HOME}/.config/nvim" ]; then
  if [ ! -e "${HOME}/.config" ]; then
    mkdir "${HOME}/.config"
  fi
  ln -sv "${HOME}/dotfiles/.config/nvim" "${HOME}/.config"
fi

# vscode
vscode_files=( snippets keybindings.json settings.json )
vscode_user_path="${HOME}/Library/Application Support/Code/User"
for f in ${vscode_files[@]}
do
  if [ -e "${vscode_user_path}" ] && [ ! -e "${vscode_user_path}/${f}" ]; then
    ln -sv "${HOME}/dotfiles/.vscode/${f}" "${vscode_user_path}"
  fi
done
