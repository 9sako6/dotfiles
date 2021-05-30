#!/bin/bash
# set -e

echo "GITHUB_REF: ${GITHUB_REF}" # for debug

[ ${GITHUB_REF} ] && branch_name=$(echo "${GITHUB_REF##*/}") || branch_name='master'

echo "branch_name: ${branch_name}" # for debug

# Test for deploy scripts
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/init.sh)"

if [ ! -L "${HOME}/.zsh.local" ]; then
  echo "Error: ${HOME}/.zsh.local is not a symlink"
  exit 1
fi

bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/clean.sh)"
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/${branch_name}/etc/deploy.sh)"

if [ ! -L "${HOME}/.zsh.local" ]; then
  echo "Error: ${HOME}/.zsh.local is not a symlink"
  exit 1
fi
