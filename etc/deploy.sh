#!/bin/bash
# set -e

source "${HOME}/dotfiles/etc/env.sh"
source "${DOTFILES_PATH}/etc/utils.sh"

function make_symlink() {
  fullpath="${DOTFILES_PATH}/dist/${1}"
  filename=$(basename "${fullpath}")

  if [ -e "${HOME}/${filename}" ]; then
    print_info "${filename}: directory already exists in ${HOME}"
  else
    ln -s "${fullpath}" "${HOME}/${filename}"
    print_success "${filename}: deployed"
  fi
}

# NOTE: xargs に関数を渡すために `export -f` が必要。
export -f make_symlink
export -f print_success
export -f print_info

ls -A -1 "${DOTFILES_PATH}/dist" | xargs -I{} bash -c "make_symlink {}"
