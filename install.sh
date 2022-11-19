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

# Install fzf
if [ ! -d "${HOME}"/.fzf ]; then
  print_info "Start to install fzf"
  mkdir -p "${HOME}"/.fzf
  git clone --depth 1 https://github.com/junegunn/fzf.git "${HOME}"/.fzf
  "${HOME}"/.fzf/install
  print_info "Finish to install fzf"
fi

cd "${HOME}/dotfiles"

# Install Deno
export DENO_INSTALL="${HOME}/.local"
export PATH="${DENO_INSTALL}/bin:${PATH}"
curl -fsSL https://deno.land/x/install/install.sh | sh

deno run --allow-write --allow-read --allow-env main.ts
ls -la ~/
