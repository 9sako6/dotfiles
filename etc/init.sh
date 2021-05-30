#!/bin/bash
# set -e

source "${HOME}/dotfiles/etc/env.sh"
source "${DOTFILES_PATH}/etc/utils.sh"

if [ ! -d "${HOME}"/dotfiles ]; then
  print_info "Start to clone dotfiles repository"
  cd "${HOME}"
  git clone https://github.com/9sako6/dotfiles
  print_info "Finish to clone dotfiles repository"
fi

# Change login shell to zsh
print_info "Start to change login shell to zsh"
chsh -s /bin/zsh
print_info "Finish to change login shell to zsh"

# Install zinit
if [ ! -d "${HOME}"/.zinit ]; then
  print_info "Start to install zinit"
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/zdharma/zinit/master/doc/install.sh)"
  print_info "Finish to install zinit"
fi

# Install fzf
if [ ! -d "${HOME}"/.fzf ]; then
  print_info "Start to install fzf"
  mkdir -p "${HOME}"/.fzf
  git clone --depth 1 https://github.com/junegunn/fzf.git "${HOME}"/.fzf
  "${HOME}"/.fzf/install
  print_info "Finish to install fzf"
fi

# Deploy dotfiles
print_info "Start to deploy dotfiles"
bash "${DOTFILES_PATH}/etc/deploy.sh"
print_info "Finish to deploy dotfiles"

# Settings for git
print_info "Start to set configs for git"
git config --global core.excludesfile ~/.gitignore_global
git config --global core.commentchar '~'
print_info "Finish to set configs for git"

# Setting for VSCode
if type code >/dev/null 2>&1; then
  print_info "Start to set configs for VSCode"
  bash "${DOTFILES_PATH}/etc/vscode/init.sh"
  print_info "Finish to set configs for VSCode"
fi

exec ${SHELL} -l
