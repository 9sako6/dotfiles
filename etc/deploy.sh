#!/bin/bash
# set -e

source "${HOME}/dotfiles/etc/env.sh"
source "${DOTFILES_PATH}/etc/utils.sh"

function make_symlink() {
  fullpath="${DOTFILES_PATH}/dist/${1}"
  filename=$(basename "${fullpath}")

  if [ ${filename} = '.vscode' ]; then
    # VSCode インストール前に ~/.vscode ディレクトリを作成すると VSCode が起動しなかった。
    # https://github.com/9sako6/dotfiles/issues/6
    if type code >/dev/null 2>&1; then
      :
    else
      print_info 'Please install VSCode and code command.'
      exit
    fi
  fi

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
