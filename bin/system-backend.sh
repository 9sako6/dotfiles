#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(/usr/bin/dirname -- "$0")" && pwd)"
. "${script_dir}/../lib/install-system.sh"

operation="${1:-}"
shift
case "$operation" in
  ensure-nix) install_system_ensure_lix "${script_dir}/install-lix.sh" ;;
  require-nix) install_system_require_lix ;;
  preview)
    nix_bin="$1"
    system_path="$2"
    brewfile_path="$3"
    current_system="${4:-/run/current-system}"
    printf 'system generation: %s\n' "$system_path"
    if [ -e "$current_system" ]; then
      "$nix_bin" --extra-experimental-features 'nix-command flakes' store diff-closures "$current_system" "$system_path"
    else
      printf 'system diff: no active nix-darwin generation\n'
    fi
    if brew_bin="$(command -v brew 2>/dev/null)"; then
      install_system_show_homebrew_configuration_changes "$nix_bin" "$brew_bin" "$current_system" "$brewfile_path"
      printf 'Homebrew package changes:\n'
      install_system_show_homebrew_missing "$brew_bin" "$brewfile_path"
      printf 'Homebrew cleanup candidates:\n'
      install_system_show_homebrew_cleanup "$brew_bin" "$brewfile_path"
    else
      printf 'Homebrew is not active yet\n'
    fi
    ;;
  activate)
    nix_bin="$1"
    primary_user="$2"
    system_path="$3"
    expected_target="$4"
    desired_target="$5"
    shift 5
    [ "$primary_user" = "$(/usr/bin/id -un)" ] || install_system_fail 'run as the login user'
    install_system_host_platform "$(/usr/bin/uname -s)" "$(/usr/bin/uname -m)" >/dev/null
    sudo_bin="$(install_system_resolve_sudo)" || install_system_fail 'trusted sudo not found'
    env_bin="$(install_system_resolve_env)" || install_system_fail 'trusted env not found'
    install_system_apply_built_system "$sudo_bin" "$env_bin" "$nix_bin" "$primary_user" "$system_path" \
      /etc/nix-darwin/flake.nix "$expected_target" "$desired_target" "$@"
    ;;
  *) install_system_fail 'unknown backend operation' ;;
esac
