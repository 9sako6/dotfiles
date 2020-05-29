#!/bin/bash
export DOTFILES_PATH="${HOME}/dotfiles"

while read file; do
  if [ -d ${file} ] && [ ! -e "${HOME}/${file}" ]; then
    ln -sv "${DOTFILES_PATH}/dist/${file}" "${HOME}"
  else
    if [ ! -e "${HOME}/${file}" ]; then
      ln -sv "${DOTFILES_PATH}/dist/${file}" "${HOME}/${file}"
    fi
  fi
done < <(ls -A -1 "${DOTFILES_PATH}/dist")

# git
bash "${DOTFILES_PATH}/etc/git/init.sh"

# nvim
bash "${DOTFILES_PATH}/etc/nvim/init.sh"

# vscode
if type code >/dev/null 2>&1; then
  bash "${DOTFILES_PATH}/etc/vscode/init.sh"
fi
