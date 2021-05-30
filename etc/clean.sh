#!/bin/bash
# set -e

source "${HOME}/dotfiles/etc/env.sh"
source "${DOTFILES_PATH}/etc/utils.sh"

function unlink_symlink() {
  fullpath="${DOTFILES_PATH}/dist/${1}"
  filename=$(basename "${fullpath}")

  if [ -e "${HOME}/${filename}" ]; then
    if [ -L "${HOME}/${filename}" ]; then
      unlink "${HOME}/${filename}"
      print_success "${filename}: symlink was unlinked"
    else
      print_info "${filename}: can't delete it because it is not a symlink"
    fi
  fi
}

# NOTE: xargs に関数を渡すために `export -f` が必要。
export -f unlink_symlink
export -f print_success
export -f print_info

ls -A -1 "${DOTFILES_PATH}/dist" | xargs -I{} bash -c "unlink_symlink {}"
