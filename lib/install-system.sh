#!/bin/sh

install_system_fail() {
  printf 'system: %s\n' "$*" >&2
  exit 1
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

install_system_resolve_lix() {
  if nix_bin="$(install_system_resolve_nix)"; then
    :
  else
    return $?
  fi

  nix_version="$("$nix_bin" --version 2>/dev/null)" || return 3
  case "$nix_version" in
    "nix (Lix, like Nix) "[0-9]* | "nix (Lix) "[0-9]*) ;;
    *) return 3 ;;
  esac

  printf '%s\n' "$nix_bin"
}

install_system_fail_lix_resolution() {
  case "$1" in
    1) install_system_fail "trusted Lix installation was not found" ;;
    2) install_system_fail "refusing untrusted Nix installation" ;;
    *) install_system_fail "system configuration requires a working Lix installation" ;;
  esac
}

install_system_ensure_lix() {
  install_lix_script="$1"

  if nix_bin="$(install_system_resolve_lix)"; then
    printf '%s\n' "$nix_bin"
    return 0
  else
    resolve_status=$?
  fi

  case "$resolve_status" in
    1) ;;
    *) install_system_fail_lix_resolution "$resolve_status" ;;
  esac

  "$install_lix_script"
  if nix_bin="$(install_system_resolve_lix)"; then
    :
  else
    install_system_fail_lix_resolution "$?"
  fi
  printf '%s\n' "$nix_bin"
}
