#!/bin/bash
# set -e

function print_error() {
  echo -e "\033[0;31mError: ${1}\033[0m"
}
function print_success() {
  echo -e "\033[0;32mSuccess: ${1}\033[0m"
}

export DOTFILES_PATH="${HOME}/dotfiles"

function make_symlink() {
  fullpath="${DOTFILES_PATH}/dist/${1}"
  filename=$(basename "${fullpath}")

  if [ -e "${HOME}/${filename}" ]; then
    print_error "${filename}: directory already exists in ${HOME}"
  else
    ln -sv "${fullpath}" "${HOME}/${filename}"
    print_success "${filename}: deployed"
  fi
}

# NOTE: xargs に関数を渡すために `export -f` が必要。
export -f make_symlink
export -f print_error
export -f print_success

ls -A -1 "${DOTFILES_PATH}/dist" | xargs -I{} bash -c "make_symlink {}"
