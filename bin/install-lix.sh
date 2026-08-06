#!/bin/sh
set -eu

LIX_INSTALLER_SHA256="3c71fdcfeddac8fa075b626b6e0ddd9ba73af930e47b4fa027e22c7279f596ae"
LIX_INSTALLER_URL="https://install.lix.systems/lix/lix-installer-aarch64-darwin"
LIX_INSTALLER_VERSION="3.95.0"

temp_dir=""

fail() {
  printf 'system: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [ -n "$temp_dir" ]; then
    rm -f "${temp_dir}/lix-installer"
    rmdir "$temp_dir" 2>/dev/null || true
  fi
}

trap cleanup 0
trap 'exit 1' HUP INT TERM

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
