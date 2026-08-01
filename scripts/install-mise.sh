#!/bin/sh
set -eu

MISE_BIN="${HOME}/.local/bin/mise"
MISE_INSTALLER_SHA256="0b98c2dc48edc807be860a76e14209afcfe36684c591f92337c5d9ff909e7740"
MISE_INSTALLER_URL="https://github.com/jdx/mise/releases/download/v2026.7.7/install.sh"
MISE_VERSION="2026.7.7"

temp_dir=""

fail() {
  printf 'install:mise: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [ -n "$temp_dir" ]; then
    rm -f "${temp_dir}/install.sh"
    rmdir "$temp_dir" 2>/dev/null || true
  fi
}

installed_version() {
  if [ -x "$MISE_BIN" ]; then
    "$MISE_BIN" --version | awk '{print $1}'
  fi
}

if [ "$(installed_version)" = "$MISE_VERSION" ]; then
  exit 0
fi

trap cleanup 0
trap 'exit 1' HUP INT TERM
umask 077

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/dotfiles-mise.XXXXXX")"
installer_path="${temp_dir}/install.sh"

curl \
  --proto '=https' \
  --tlsv1.2 \
  --silent \
  --show-error \
  --fail \
  --location \
  --output "$installer_path" \
  "$MISE_INSTALLER_URL"

printf '%s  %s\n' "$MISE_INSTALLER_SHA256" "$installer_path" |
  shasum -a 256 -c -

MISE_INSTALL_PATH="$MISE_BIN" MISE_VERSION="v${MISE_VERSION}" sh "$installer_path"

actual_version="$(installed_version)"
[ "$actual_version" = "$MISE_VERSION" ] ||
  fail "expected mise ${MISE_VERSION}, got: ${actual_version:-not installed}"

cleanup
temp_dir=""
