#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_dir="$(dirname -- "$script_dir")"
darwin_dir="${repo_dir}/darwin"
install_lix_script="${script_dir}/install-lix.sh"

fail() {
  printf 'install:system: %s\n' "$*" >&2
  exit 1
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
  "$install_lix_script"
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
