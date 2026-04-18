#!/bin/sh
set -eu

DOTFILES_DIR="${DOTFILES_DIR:-$HOME/dotfiles}"
DOTFILES_REPO_URL="${DOTFILES_REPO_URL:-https://github.com/9sako6/dotfiles.git}"
MISE_BIN="${HOME}/.local/bin/mise"

install_homebrew() {
  NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
}

install_mise() {
  curl -fsSL https://mise.run | sh
}

if [ ! -d "$DOTFILES_DIR/.git" ]; then
  git clone "$DOTFILES_REPO_URL" "$DOTFILES_DIR"
fi

if ! command -v brew >/dev/null 2>&1; then
  install_homebrew
fi

if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
else
  eval "$(/usr/local/bin/brew shellenv)"
fi

if [ ! -x "$MISE_BIN" ]; then
  install_mise
fi

cd "$DOTFILES_DIR"
"$MISE_BIN" trust
# Install the repo's runtime tools first so the repo tasks can execute.
"$MISE_BIN" install
"$MISE_BIN" run install:user
"$MISE_BIN" run agents:build
"$MISE_BIN" run apply
