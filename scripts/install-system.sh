#!/bin/sh
set -eu

LIX_INSTALLER_SHA256="3c71fdcfeddac8fa075b626b6e0ddd9ba73af930e47b4fa027e22c7279f596ae"
LIX_INSTALLER_URL="https://install.lix.systems/lix/lix-installer-aarch64-darwin"
LIX_INSTALLER_VERSION="3.95.0"

repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
darwin_dir="${repo_dir}/darwin"
temp_dir=""

fail() {
  printf 'install:system: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [ -n "$temp_dir" ]; then
    rm -f "${temp_dir}/lix-installer"
    rmdir "$temp_dir" 2>/dev/null || true
  fi
}

find_nix() {
  if command -v nix >/dev/null 2>&1; then
    command -v nix
  elif [ -x /nix/var/nix/profiles/default/bin/nix ]; then
    printf '%s\n' /nix/var/nix/profiles/default/bin/nix
  elif [ -x /run/current-system/sw/bin/nix ]; then
    printf '%s\n' /run/current-system/sw/bin/nix
  else
    return 1
  fi
}

install_lix() {
  temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/dotfiles-lix.XXXXXX")"
  installer_path="${temp_dir}/lix-installer"

  curl \
    --proto '=https' \
    --tlsv1.2 \
    --silent \
    --show-error \
    --fail \
    --location \
    --output "$installer_path" \
    "$LIX_INSTALLER_URL"

  printf '%s  %s\n' "$LIX_INSTALLER_SHA256" "$installer_path" |
    shasum -a 256 -c -
  chmod u+x "$installer_path"

  installer_version="$("$installer_path" --version)"
  case "$installer_version" in
    "lix-installer ${LIX_INSTALLER_VERSION}:"*) ;;
    *) fail "expected Lix Installer ${LIX_INSTALLER_VERSION}, got: ${installer_version}" ;;
  esac

  "$installer_path" install --enable-flakes --no-confirm
  cleanup
  temp_dir=""
}

trap cleanup 0
trap 'exit 1' HUP INT TERM

[ "$(uname -s)" = "Darwin" ] || fail "nix-darwin requires macOS"

case "$(uname -m)" in
  arm64) host_platform="aarch64-darwin" ;;
  *) fail "Lix system management currently supports Apple Silicon only" ;;
esac

if [ "$(id -u)" = "0" ]; then
  [ -n "${SUDO_USER:-}" ] ||
    fail "run this task as a regular user; it will request sudo itself"
  primary_user="$SUDO_USER"
else
  primary_user="$(id -un)"
fi

if ! nix_bin="$(find_nix)"; then
  install_lix
  nix_bin="$(find_nix)" ||
    fail "Lix installation completed but nix was not found"
fi

sudo env \
  DARWIN_PRIMARY_USER="$primary_user" \
  "$nix_bin" \
  --extra-experimental-features "nix-command flakes" \
  run \
  --impure \
  "path:${darwin_dir}#darwin-rebuild" \
  -- \
  switch \
  --flake "path:${darwin_dir}#${host_platform}" \
  --impure
