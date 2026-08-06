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

install_system_apply_built_system() (
  sudo_bin="$1"
  env_bin="$2"
  nix_bin="$3"
  primary_user="$4"
  system_path="$5"
  selection_path="$6"
  expected_target="$7"
  desired_target="$8"

  nix_env_bin="${nix_bin%/nix}/nix-env"
  rebuild_bin="${system_path}/sw/bin/darwin-rebuild"

  [ -x "$nix_env_bin" ] || install_system_fail "built Lix has no nix-env"
  [ -x "$rebuild_bin" ] || install_system_fail "built system has no darwin-rebuild"
  "$sudo_bin" "$env_bin" SUDO_USER="$primary_user" /bin/sh -eu -c '
    nix_env_bin="$1"
    rebuild_bin="$2"
    system_path="$3"
    selection_path="$4"
    expected_target="$5"
    desired_target="$6"

    selection_dir="$(/usr/bin/dirname -- "$selection_path")"
    lock_path="${selection_path}.apply.lock"
    lock_candidate="${lock_path}.$$.candidate"
    /bin/mkdir -p -- "$selection_dir"
    while :; do
      /bin/rm -f -- "$lock_candidate"
      printf "%s\n" "$$" > "$lock_candidate"
      if /bin/ln -- "$lock_candidate" "$lock_path" 2>/dev/null; then
        /bin/rm -f -- "$lock_candidate"
        break
      fi
      /bin/rm -f -- "$lock_candidate"
      [ -e "$lock_path" ] || {
        printf "system: could not acquire system apply lock\n" >&2
        exit 1
      }

      lock_owner="$(/bin/cat -- "$lock_path" 2>/dev/null || :)"
      case "$lock_owner" in
        "" | *[!0-9]*) ;;
        *)
          if /bin/kill -0 "$lock_owner" 2>/dev/null; then
            printf "system: system apply is already running\n" >&2
            exit 1
          fi
          ;;
      esac

      stale_lock="${lock_path}.$$.stale"
      if /bin/mv -- "$lock_path" "$stale_lock" 2>/dev/null; then
        /bin/rm -f -- "$stale_lock"
      fi
    done
    install_system_release_apply_lock() {
      lock_owner="$(/bin/cat -- "$lock_path" 2>/dev/null || :)"
      if [ "$lock_owner" = "$$" ]; then
        /bin/rm -f -- "$lock_path"
      fi
    }
    trap install_system_release_apply_lock 0
    trap "exit 1" HUP INT TERM

    "$nix_env_bin" -p /nix/var/nix/profiles/system --set "$system_path"
    "$rebuild_bin" activate

    if [ "$expected_target" = missing ]; then
      if [ -e "$selection_path" ] || [ -L "$selection_path" ]; then
        printf "system: system source selection changed during apply\n" >&2
        exit 1
      fi
    else
      if [ ! -L "$selection_path" ] ||
        [ "$(/usr/bin/readlink -- "$selection_path")" != "$expected_target" ]
      then
        printf "system: system source selection changed during apply\n" >&2
        exit 1
      fi
    fi

    temporary_path="${selection_dir}/.flake.nix.$$"
    /bin/ln -s -- "$desired_target" "$temporary_path" || {
      printf "system: could not stage system source selection\n" >&2
      exit 1
    }
    if ! /bin/mv -f -- "$temporary_path" "$selection_path"; then
      /bin/rm -f -- "$temporary_path"
      printf "system: could not persist system source selection\n" >&2
      exit 1
    fi
  ' install-system-apply \
    "$nix_env_bin" "$rebuild_bin" "$system_path" "$selection_path" \
    "$expected_target" "$desired_target"
)
