#!/bin/sh

install_system_fail() {
  printf 'system: %s\n' "$*" >&2
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

install_system_require_lix() {
  if nix_bin="$(install_system_resolve_lix)"; then
    printf '%s\n' "$nix_bin"
    return 0
  else
    resolve_status=$?
  fi

  install_system_fail_lix_resolution "$resolve_status"
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

install_system_build_source_output() {
  nix_bin="$1"
  primary_user="$2"
  source_dir="$3"
  source_kind="$4"
  output="$5"

  case "$source_kind" in
    default)
      DARWIN_PRIMARY_USER="$primary_user" \
        "$nix_bin" \
        --extra-experimental-features "nix-command flakes" \
        build \
        --impure \
        --no-link \
        --print-out-paths \
        "path:${source_dir}#${output}"
      ;;
    remote)
      "$nix_bin" \
        --extra-experimental-features "nix-command flakes" \
        build \
        --no-link \
        --print-out-paths \
        "path:${source_dir}#${output}"
      ;;
    *) install_system_fail "unknown system source kind: $source_kind" ;;
  esac
}

install_system_confirm_apply() {
  printf 'Apply this system plan? Type yes: '
  if ! IFS= read -r answer || [ "$answer" != yes ]; then
    install_system_fail "system apply cancelled"
  fi
}

install_system_start_sudo_refresh() {
  sudo_bin="$1"
  "$sudo_bin" -v
  install_system_sudo_refresh_owner=$$
  (
    trap 'exit 0' HUP INT TERM
    refresh_after=0
    while /bin/kill -0 "$install_system_sudo_refresh_owner" 2>/dev/null; do
      if [ "$refresh_after" -eq 0 ]; then
        "$sudo_bin" -n -v >/dev/null 2>&1 || exit
        refresh_after=60
      fi
      /bin/sleep 1
      refresh_after=$((refresh_after - 1))
    done
  ) &
  install_system_sudo_refresh_pid=$!
}

install_system_stop_sudo_refresh() {
  [ -n "${install_system_sudo_refresh_pid:-}" ] || return 0
  /bin/kill "$install_system_sudo_refresh_pid" 2>/dev/null || true
  wait "$install_system_sudo_refresh_pid" 2>/dev/null || true
  install_system_sudo_refresh_pid=
}

install_system_show_homebrew_cleanup() {
  brew_bin="$1"
  brewfile_path="$2"
  cleanup_status=0
  cleanup_output="$(
    HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" bundle cleanup --file "$brewfile_path" 2>&1
  )" || cleanup_status=$?
  [ -z "$cleanup_output" ] || printf '%s\n' "$cleanup_output"
  case "$cleanup_status" in
    0 | 1) ;;
    *) install_system_fail "Homebrew cleanup plan failed" ;;
  esac
}

install_system_activate_built_system() {
  sudo_bin="$1"
  env_bin="$2"
  nix_bin="$3"
  primary_user="$4"
  system_path="$5"
  nix_env_bin="${nix_bin%/nix}/nix-env"
  rebuild_bin="${system_path}/sw/bin/darwin-rebuild"

  [ -x "$nix_env_bin" ] || install_system_fail "built Lix has no nix-env"
  [ -x "$rebuild_bin" ] || install_system_fail "built system has no darwin-rebuild"
  "$sudo_bin" "$nix_env_bin" \
    -p /nix/var/nix/profiles/system \
    --set "$system_path"
  "$sudo_bin" "$env_bin" \
    SUDO_USER="$primary_user" \
    "$rebuild_bin" activate
}

install_system_apply_built_system() (
  sudo_bin="$1"
  env_bin="$2"
  nix_bin="$3"
  primary_user="$4"
  system_path="$5"
  selection_path="$6"
  expected_target="$7"
  desired_target="$8"

  install_system_start_sudo_refresh "$sudo_bin"
  trap 'install_system_stop_sudo_refresh' 0
  trap 'exit 1' HUP INT TERM
  install_system_activate_built_system \
    "$sudo_bin" "$env_bin" "$nix_bin" "$primary_user" "$system_path"
  install_system_select_source \
    "$sudo_bin" "$selection_path" "$expected_target" "$desired_target"
  install_system_stop_sudo_refresh
  trap - 0 HUP INT TERM
)

install_system_select_source() {
  sudo_bin="$1"
  selection_path="$2"
  expected_target="$3"
  desired_target="$4"

  if [ "$expected_target" = missing ]; then
    [ ! -e "$selection_path" ] && [ ! -L "$selection_path" ] ||
      install_system_fail "system source selection changed during apply"
  else
    [ -L "$selection_path" ] ||
      install_system_fail "system source selection changed during apply"
    [ "$(/usr/bin/readlink -- "$selection_path")" = "$expected_target" ] ||
      install_system_fail "system source selection changed during apply"
  fi

  selection_dir="$(/usr/bin/dirname -- "$selection_path")"
  temporary_path="${selection_dir}/.flake.nix.$$"
  "$sudo_bin" /bin/mkdir -p -- "$selection_dir"
  "$sudo_bin" /bin/ln -s -- "$desired_target" "$temporary_path" ||
    install_system_fail "could not stage system source selection"
  if ! "$sudo_bin" /bin/mv -f -- "$temporary_path" "$selection_path"; then
    "$sudo_bin" /bin/rm -f -- "$temporary_path"
    install_system_fail "could not persist system source selection"
  fi
}
