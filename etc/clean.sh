#!/bin/bash
# set -e

function print_error() {
  echo -e "\033[0;31mError: ${1}\033[0m"
}
function print_success() {
  echo -e "\033[0;32mSuccess: ${1}\033[0m"
}

export DOTFILES_PATH="${HOME}/dotfiles"

function unlink_symlink() {
  fullpath="${DOTFILES_PATH}/dist/${1}"
  filename=$(basename "${fullpath}")

  if [ -L "${HOME}/${filename}" ]; then
    unlink "${HOME}/${filename}"
    print_success "${filename}: symlink was unlinked"
  else
    print_error "${filename}: can't delete it because it is not a symlink"
  fi
}

# NOTE: xargs に関数を渡すために `export -f` が必要。
export -f unlink_symlink
export -f print_error
export -f print_success

ls -A -1 "${DOTFILES_PATH}/dist" | xargs -I{} bash -c "unlink_symlink {}"
