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

install_system_show_homebrew_configuration_changes() (
  nix_bin="$1"
  brew_bin="$2"
  current_system="$3"
  brewfile_path="$4"
  [ -e "$current_system" ] || return 0

  references="$("${nix_bin%/nix}/nix-store" --query --references "$current_system")" ||
    install_system_fail "active Homebrew configuration inspection failed"
  previous_brewfile="$(printf '%s\n' "$references" | awk '/-Brewfile$/')"
  case "$previous_brewfile" in
    "") return 0 ;;
    *'
'*) install_system_fail "active system references multiple Brewfiles" ;;
  esac
  [ "$previous_brewfile" != "$brewfile_path" ] || return 0

  shown=0
  for kind in formula cask tap; do
    previous="$(HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" bundle list --"$kind" --file "$previous_brewfile")" ||
      install_system_fail "previous Homebrew $kind declarations could not be read"
    desired="$(HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" bundle list --"$kind" --file "$brewfile_path")" ||
      install_system_fail "planned Homebrew $kind declarations could not be read"
    changes="$(DOTFILES_PLAN_PREVIOUS="$previous" DOTFILES_PLAN_DESIRED="$desired" awk -v kind="$kind" '
      BEGIN {
        count = split(ENVIRON["DOTFILES_PLAN_PREVIOUS"], names, "\n")
        for (i = 1; i <= count; i++) if (names[i] != "") previous[names[i]] = 1
        count = split(ENVIRON["DOTFILES_PLAN_DESIRED"], names, "\n")
        for (i = 1; i <= count; i++) if (names[i] != "") desired[names[i]] = 1
        for (name in previous) if (!(name in desired)) printf "  - %s (%s)\n", name, kind
        for (name in desired) if (!(name in previous)) printf "  + %s (%s)\n", name, kind
      }
    ')" || install_system_fail "Homebrew configuration comparison failed"
    if [ -n "$changes" ]; then
      if [ "$shown" -eq 0 ]; then
        printf 'Homebrew configuration changes:\n'
        shown=1
      fi
      printf '%s\n' "$changes" | LC_ALL=C sort
    fi
  done
)

install_system_show_homebrew_missing() {
  brew_bin="$1"
  brewfile_path="$2"
  check_status=0
  check_output="$(
    HOMEBREW_NO_AUTO_UPDATE=1 \
      "$brew_bin" bundle check --verbose --no-upgrade --file "$brewfile_path" 2>&1
  )" || check_status=$?
  case "$check_status" in
    0 | 1) ;;
    *)
      printf '%s\n' "$check_output" >&2
      install_system_fail "Homebrew dependency plan failed"
      ;;
  esac
  installed_formulae="$(HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" list --formula --full-name)" ||
    install_system_fail "Homebrew installed formula inspection failed"
  installed_casks="$(HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" list --cask --full-name)" ||
    install_system_fail "Homebrew installed cask inspection failed"
  printf '%s\n' "$check_output" | DOTFILES_PLAN_FORMULAE="$installed_formulae" \
    DOTFILES_PLAN_CASKS="$installed_casks" awk '
    function record(values, kind, names, count, i, name) {
      count = split(values, names, "\n")
      for (i = 1; i <= count; i++) {
        name = names[i]
        installed[kind, name] = 1
        sub(/^.*\//, "", name)
        installed[kind, name] = 1
      }
    }
    BEGIN {
      record(ENVIRON["DOTFILES_PLAN_FORMULAE"], "Formula")
      record(ENVIRON["DOTFILES_PLAN_CASKS"], "Cask")
    }
    /^→ (Cask|Formula) [^ ]+ needs to be installed( or updated)?\.$/ {
      kind = $2
      name = $3
      action = installed[kind, name] ? "~ Update" : "+ Install"
      printf "  %s %s (%s)\n", action, name, tolower(kind)
      shown++
      next
    }
    /^brew bundle can.t satisfy your Brewfile/ { next }
    /^Satisfy missing dependencies with/ { next }
    /^The Brewfile.s dependencies are satisfied\.$/ { next }
    NF { print; shown++ }
    END { if (!shown) print "  No package changes" }
  '
}

install_system_show_homebrew_cleanup() {
  brew_bin="$1"
  brewfile_path="$2"
  cleanup_status=0
  cleanup_output="$(
    HOMEBREW_NO_AUTO_UPDATE=1 "$brew_bin" bundle cleanup --file "$brewfile_path" 2>&1
  )" || cleanup_status=$?
  case "$cleanup_status" in
    0 | 1) ;;
    *)
      printf '%s\n' "$cleanup_output" >&2
      install_system_fail "Homebrew cleanup plan failed"
      ;;
  esac
  printf '%s\n' "$cleanup_output" | awk '
    /^Would `brew cleanup`:/ { next }
    /^Would remove: / { cleanup++; next }
    /^Run `brew bundle cleanup --force` to make these changes\.$/ { next }
    NF { print; shown++ }
    END {
      if (cleanup) printf "  Cache and old-version cleanup: %d entries\n", cleanup
      else if (!shown) print "  No cleanup candidates"
    }
  '
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
  shift 8

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

    install_system_verify_record() {
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

    }
    install_system_verify_record

    "$nix_env_bin" -p /nix/var/nix/profiles/system --set "$system_path"
    "$rebuild_bin" activate
    shift 6
    if [ "$#" -gt 0 ]; then
      cli_bin="$1"
      source="$2"
      paths="$3"
      home_directory="$4"
      /usr/bin/sudo --user="$SUDO_USER" -- "$cli_bin" complete-apply "$source" "$paths" "$home_directory"
    fi

    install_system_verify_record
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
    "$expected_target" "$desired_target" "$@"
)
