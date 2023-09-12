if [ ! -d "${HOME}/dotfiles" ]; then
  echo "Start to clone dotfiles repository"
  cd "${HOME}"
  if [ ${CI} ]; then
    # in GitHub Actions
    git clone -b ${CURRENT_BRANCH_NAME} https://github.com/9sako6/dotfiles
  else
    git clone https://github.com/9sako6/dotfiles
  fi
  echo "Finish to clone dotfiles repository"
fi

cd "${HOME}/dotfiles"

# Install Deno
export DENO_INSTALL="${HOME}/.local"
export PATH="${DENO_INSTALL}/bin:${PATH}"
curl -fsSL https://deno.land/x/install/install.sh | sh

deno run --allow-write --allow-read --allow-env main.ts

# Install rtx
curl https://rtx.pub/install.sh | sh

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
