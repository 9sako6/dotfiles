#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(/usr/bin/dirname -- "$0")" && pwd)"
. "${script_dir}/../lib/install-system.sh"

[ "$(/usr/bin/id -u)" != 0 ] ||
  install_system_fail "run rollback as the login user; it will request sudo itself"
rebuild_bin=/run/current-system/sw/bin/darwin-rebuild
[ -x "$rebuild_bin" ] || install_system_fail "no active nix-darwin generation was found"
sudo_bin="$(install_system_resolve_sudo)" ||
  install_system_fail "trusted /usr/bin/sudo was not found"
"$sudo_bin" "$rebuild_bin" --rollback
