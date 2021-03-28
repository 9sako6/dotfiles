#!/bin/bash
export DOTFILES_PATH="${HOME}/dotfiles"

# set -e

echo '     _       _    __ _ _            '
echo '    | |     | |  / _(_) |           '
echo '  __| | ___ | |_| |_ _| | ___  ___  '
echo ' / _` |/ _ \| __|  _| | |/ _ \/ __| '
echo '| (_| | (_) | |_| | | | |  __/\__ \ '
echo ' \__,_|\___/ \__|_| |_|_|\___||___/ '
echo ''

# utils
function print_error() {
  echo -e "\033[0;31mERROR: ${1}\033[0m"
}
function print_success() {
  echo -e "\033[0;32mSuccess: ${1}\033[0m"
}
function print_info() {
  echo -e "\033[0;34mINFO: ${1}\033[0m"
}

if ! $git_exists || ! $vim_exists || ! $zsh_exists; then
  echo -e "\033[0;31mError: git or vim or zsh is not installed.\033[0m"
  echo -e "\033[0;31mPlease install them\033[0m"
  exit
fi

if [ ! -d "${HOME}"/dotfiles ]; then
  cd "${HOME}"
  git clone https://github.com/9sako6/dotfiles
fi

# zinit
if [ ! -d "${HOME}"/.zinit ]; then
  print_info "install zinit"
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/zdharma/zinit/master/doc/install.sh)"
fi

# fzf
if [ ! -d "${HOME}"/.fzf ]; then
  print_info "install fzf"
  mkdir -p "${HOME}"/.fzf
  git clone --depth 1 https://github.com/junegunn/fzf.git "${HOME}"/.fzf
  "${HOME}"/.fzf/install
fi

# PowerLine font
if [ ! -e "${HOME}/.cache/powerline" ]; then
  git clone https://github.com/powerline/fonts.git "${HOME}/.cache/powerline"
  sh "${HOME}/.cache/powerline/install.sh"
fi

# complete messages
print_success "dotfiles are successfully initialized!"
print_info "To finish vim settings, do :PlugInstall in vim console"
