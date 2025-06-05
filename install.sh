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

# Install zinit
bash -c "$(curl --fail --show-error --silent --location https://raw.githubusercontent.com/zdharma-continuum/zinit/HEAD/scripts/install.sh)"

source ~/.zshenv
source ~/.zshrc

# Install mise
curl https://mise.jdx.dev/install.sh | sh

mise install
