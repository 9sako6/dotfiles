#!/bin/sh
set -eu

MISE_BIN="${HOME}/.local/bin/mise"
MISE_VERSION="2026.7.7"

if [ ! -x "$MISE_BIN" ] || [ "$("$MISE_BIN" --version | awk '{print $1}')" != "$MISE_VERSION" ]; then
  curl -fsSL https://mise.run | MISE_VERSION="v${MISE_VERSION}" sh
fi
