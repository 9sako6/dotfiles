#!/bin/bash
# set -e

function test_dist_files () {
  if [ ! -L "${HOME}/.zsh.local" ]; then
    echo "Error: ${HOME}/.zsh.local is not a symlink"
    exit 1
  fi

  if [ ! -L "${HOME}/mybin" ]; then
    echo "Error: ${HOME}/mybin is not a symlink"
    exit 1
  fi

  if [ ! -d "${HOME}/.zinit" ]; then
    echo "Error: ${HOME}/.zinit doesn't exist"
    exit 1
  fi

  if [ ! -f "${HOME}/.vim/autoload/plug.vim" ]; then
    echo "Error: ${HOME}/.vim/autoload/plug.vim doesn't exist"
    exit 1
  fi

  fzf --version
}

echo "GITHUB_REF: ${GITHUB_REF}" # for debug

[ ${GITHUB_REF} ] && branch_name=$(echo "${GITHUB_REF##*/}") || branch_name='master'

echo "branch_name: ${branch_name}" # for debug

# Test for deploy scripts
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/init.sh)"

test_dist_files

bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/clean.sh)"
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/deploy.sh)"

test_dist_files
