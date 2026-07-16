#!/bin/sh
set -eu

DOTFILES_DIR="${DOTFILES_DIR:-$HOME/dotfiles}"
DOTFILES_REPO_URL="${DOTFILES_REPO_URL:-https://github.com/9sako6/dotfiles.git}"
MISE_BIN="${HOME}/.local/bin/mise"
MISE_VERSION="2026.7.7"

install_mise() {
  curl -fsSL https://mise.run | MISE_VERSION="v${MISE_VERSION}" sh
}

if [ ! -d "$DOTFILES_DIR/.git" ]; then
  git clone "$DOTFILES_REPO_URL" "$DOTFILES_DIR"
fi

if [ ! -x "$MISE_BIN" ] || [ "$("$MISE_BIN" --version | awk '{print $1}')" != "$MISE_VERSION" ]; then
  install_mise
fi

cd "$DOTFILES_DIR"
"$MISE_BIN" trust
"$MISE_BIN" bootstrap --yes
