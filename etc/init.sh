#!/bin/bash
# set -e

if [ ! -d "${HOME}/dotfiles" ]; then
  echo "Start to clone dotfiles repository"
  cd "${HOME}"
  if [ ${GITHUB_REF} ]; then
    # in GitHub Actions
    branch_name=$(echo "${GITHUB_REF##*/}")
    git clone -b ${branch_name} https://github.com/9sako6/dotfiles
  else
    git clone https://github.com/9sako6/dotfiles
  fi
  echo "Finish to clone dotfiles repository"
fi

source "${HOME}/dotfiles/etc/env.sh"
source "${HOME}/dotfiles/etc/utils.sh"

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

# Install vim-plug
if [ ! -f "${HOME}"/.vim/autoload/plug.vim ]; then
  print_info 'Start to install vim-plug'
  sh "${DOTFILES_PATH}"/etc/installer/vim-plug.sh
  print_info 'Please run `:PlugInstall` in vim'
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
