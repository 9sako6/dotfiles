#!/bin/sh

install_system_fail() {
  printf 'install:system: %s\n' "$*" >&2
  exit 1
}

install_system_host_platform() {
  [ "$1" = "Darwin" ] || install_system_fail "nix-darwin requires macOS"

  case "$2" in
    arm64) printf '%s\n' aarch64-darwin ;;
    *) install_system_fail "Lix system management currently supports Apple Silicon only" ;;
  esac
}

install_system_is_root_owned_readonly() {
  metadata="$(LC_ALL=C /usr/bin/stat -L -f '%u:%Lp' -- "$1" 2>/dev/null)" ||
    return 1
  owner="${metadata%%:*}"
  mode="${metadata#*:}"

  [ "$owner" = "0" ] || return 1
  [ $((0$mode & 022)) -eq 0 ] || return 1
}

install_system_resolve_sudo() {
  sudo_bin=/usr/bin/sudo
  [ -f "$sudo_bin" ] && [ -x "$sudo_bin" ] || return 1
  install_system_is_root_owned_readonly /usr || return 1
  install_system_is_root_owned_readonly /usr/bin || return 1
  install_system_is_root_owned_readonly "$sudo_bin" || return 1
  printf '%s\n' "$sudo_bin"
}

install_system_resolve_env() {
  env_bin=/usr/bin/env
  [ -f "$env_bin" ] && [ -x "$env_bin" ] || return 1
  install_system_is_root_owned_readonly /usr || return 1
  install_system_is_root_owned_readonly /usr/bin || return 1
  install_system_is_root_owned_readonly "$env_bin" || return 1
  printf '%s\n' "$env_bin"
}

install_system_resolve_nix() {
  nix_profile=/nix/var/nix/profiles/default
  nix_candidate="${nix_profile}/bin/nix"
  [ -e "$nix_candidate" ] || return 1

  nix_bin="$(/usr/bin/readlink -f -- "$nix_candidate" 2>/dev/null)" || return 2
  case "$nix_bin" in
    /nix/store/*/bin/nix) ;;
    *) return 2 ;;
  esac

  for trusted_path in \
    /nix \
    /nix/var \
    /nix/var/nix \
    /nix/var/nix/profiles \
    "$nix_profile" \
    "${nix_profile}/bin" \
    "$nix_bin"
  do
    install_system_is_root_owned_readonly "$trusted_path" || return 2
  done

  [ -f "$nix_bin" ] && [ -x "$nix_bin" ] || return 2
  printf '%s\n' "$nix_bin"
}

install_system_ensure_nix() {
  install_lix_script="$1"

  if nix_bin="$(install_system_resolve_nix)"; then
    printf '%s\n' "$nix_bin"
    return 0
  else
    resolve_status=$?
  fi

  [ "$resolve_status" -eq 1 ] ||
    install_system_fail "refusing untrusted Nix installation"

  "$install_lix_script"
  nix_bin="$(install_system_resolve_nix)" ||
    install_system_fail "Lix installation completed but trusted nix was not found"
  printf '%s\n' "$nix_bin"
}

install_system_run_darwin_rebuild() {
  sudo_bin="$1"
  env_bin="$2"
  nix_bin="$3"
  primary_user="$4"
  darwin_dir="$5"
  host_platform="$6"

  "$sudo_bin" "$env_bin" \
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
}

install_system_main() {
  script_dir="$1"
  repo_dir="$(/usr/bin/dirname -- "$script_dir")"
  darwin_dir="${repo_dir}/darwin"
  install_lix_script="${script_dir}/install-lix.sh"

  host_platform="$(
    install_system_host_platform \
      "$(/usr/bin/uname -s)" \
      "$(/usr/bin/uname -m)"
  )"

  if [ "$(/usr/bin/id -u)" = "0" ]; then
    [ -n "${SUDO_USER:-}" ] ||
      install_system_fail "run this task as a regular user; it will request sudo itself"
    primary_user="$SUDO_USER"
  else
    primary_user="$(/usr/bin/id -un)"
  fi

  sudo_bin="$(install_system_resolve_sudo)" ||
    install_system_fail "trusted /usr/bin/sudo was not found"
  env_bin="$(install_system_resolve_env)" ||
    install_system_fail "trusted /usr/bin/env was not found"
  nix_bin="$(install_system_ensure_nix "$install_lix_script")"

  install_system_run_darwin_rebuild \
    "$sudo_bin" \
    "$env_bin" \
    "$nix_bin" \
    "$primary_user" \
    "$darwin_dir" \
    "$host_platform"
}
