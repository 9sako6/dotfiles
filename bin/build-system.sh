#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(/usr/bin/dirname -- "$0")" && pwd)"
. "${script_dir}/../lib/install-system.sh"

host_platform="$(
  install_system_host_platform \
    "$(/usr/bin/uname -s)" \
    "$(/usr/bin/uname -m)"
)"
nix_bin="$(install_system_resolve_nix)" ||
  install_system_fail "trusted Nix installation was not found"
case "$("$nix_bin" --version)" in
  *Lix*) ;;
  *) install_system_fail "system configuration requires Lix" ;;
esac

install_system_run_darwin_build \
  "$nix_bin" \
  "$(/usr/bin/id -un)" \
  "$(/usr/bin/dirname -- "$script_dir")/darwin" \
  "$host_platform"
