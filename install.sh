#!/bin/sh
set -eu

DOTFILES_DIR="${DOTFILES_DIR:-$HOME/dotfiles}"
DOTFILES_REPO_URL="${DOTFILES_REPO_URL:-https://github.com/9sako6/dotfiles.git}"
MISE_BIN="${HOME}/.local/bin/mise"

if [ ! -d "$DOTFILES_DIR/.git" ]; then
  git clone "$DOTFILES_REPO_URL" "$DOTFILES_DIR"
fi

if [ ! -x "$MISE_BIN" ]; then
  curl -fsSL https://mise.run | sh
fi

cd "$DOTFILES_DIR"
"$MISE_BIN" trust
# Install the repo's runtime tools first so `mise run setup` can execute.
"$MISE_BIN" install
"$MISE_BIN" run setup
# setup links ~/.config/mise/config.toml, so install user-level tools afterwards.
"$MISE_BIN" install
