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

cd "${HOME}/dotfiles"

# Install asdf
git clone https://github.com/asdf-vm/asdf.git ~/.asdf --branch v0.12.0
source ~/.zshenv
asdf --version

# Install Deno
export DENO_INSTALL="${HOME}/.local"
export PATH="${DENO_INSTALL}/bin:${PATH}"
curl -fsSL https://deno.land/x/install/install.sh | sh

deno run --allow-write --allow-read --allow-env main.ts

# Install aqua
curl -sSfL -O https://raw.githubusercontent.com/aquaproj/aqua-installer/v2.1.1/aqua-installer
echo "c2af02bdd15da6794f9c98db40332c804224930212f553a805425441f8331665  aqua-installer" | sha256sum -c
chmod +x aqua-installer
./aqua-installer

source ~/.zshenv
source ~/.zshrc

aqua --version

# Install deps with aqua
aqua i -a
