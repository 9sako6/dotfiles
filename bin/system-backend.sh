#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(/usr/bin/dirname -- "$0")" && pwd)"
. "${script_dir}/../lib/install-system.sh"

mode="${1:-}"
source_kind="${2:-}"
primary_user="${3:-}"
source_dir="${4:-}"
expected_target="${5:-}"
desired_target="${6:-}"

case "$mode" in
  apply | plan) ;;
  *) install_system_fail "usage: system-backend.sh <plan|apply> <source> ..." ;;
esac
[ "$source_kind" = default ] || [ "$source_kind" = remote ] ||
  install_system_fail "unknown system source kind: $source_kind"
[ "$primary_user" = "$(/usr/bin/id -un)" ] ||
  install_system_fail "system tasks must run as the login user"
[ -f "${source_dir}/flake.nix" ] || install_system_fail "system source has no flake.nix"

install_system_host_platform "$(/usr/bin/uname -s)" "$(/usr/bin/uname -m)" >/dev/null
if [ "$mode" = apply ]; then
  nix_bin="$(install_system_ensure_lix "${script_dir}/install-lix.sh")"
else
  nix_bin="$(install_system_require_lix)"
fi

system_path="$(install_system_build_source_output \
  "$nix_bin" "$primary_user" "$source_dir" "$source_kind" \
  darwinConfigurations.current.system)"
brewfile_path="$(install_system_build_source_output \
  "$nix_bin" "$primary_user" "$source_dir" "$source_kind" \
  darwinConfigurations.current.homebrewBrewfile)"

printf 'system generation: %s\n' "$system_path"
if [ -e /run/current-system ]; then
  "$nix_bin" \
    --extra-experimental-features "nix-command flakes" \
    store diff-closures /run/current-system "$system_path"
else
  printf 'system diff: no active nix-darwin generation\n'
fi

if brew_bin="$(command -v brew 2>/dev/null)"; then
  printf 'Homebrew missing dependencies:\n'
  install_system_show_homebrew_missing "$brew_bin" "$brewfile_path"
  printf 'Homebrew cleanup candidates:\n'
  install_system_show_homebrew_cleanup "$brew_bin" "$brewfile_path"
else
  printf 'Homebrew missing dependencies: brew is not active yet\n'
  printf 'Homebrew cleanup candidates: brew is not active yet\n'
fi

if [ -n "${DOTFILES_HOME_COPY_PLAN:-}" ]; then
  printf '%s\n' "$DOTFILES_HOME_COPY_PLAN"
fi

[ "$mode" = apply ] || exit 0
install_system_confirm_apply

sudo_bin="$(install_system_resolve_sudo)" ||
  install_system_fail "trusted /usr/bin/sudo was not found"
env_bin="$(install_system_resolve_env)" ||
  install_system_fail "trusted /usr/bin/env was not found"
install_system_apply_built_system \
  "$sudo_bin" "$env_bin" "$nix_bin" "$primary_user" "$system_path" \
  /etc/nix-darwin/flake.nix "$expected_target" "$desired_target"
