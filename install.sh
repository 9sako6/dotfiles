#!/bin/sh
set -eu

main() {
  DOTFILES_DIR="${DOTFILES_DIR:-$HOME/dotfiles}"
  DOTFILES_REPO_URL="${DOTFILES_REPO_URL:-https://github.com/9sako6/dotfiles.git}"
  MISE_BIN="${HOME}/.local/bin/mise"

  if [ "$(/usr/bin/basename -- "$0")" = sh ] && [ ! -t 0 ]; then
    [ -r /dev/tty ] || {
      printf 'install: a control terminal is required for confirmation\n' >&2
      exit 1
    }
    exec </dev/tty
  fi

  if [ ! -d "$DOTFILES_DIR/.git" ]; then
    git clone --no-checkout "$DOTFILES_REPO_URL" "$DOTFILES_DIR"
  else
    [ -z "$(git -C "$DOTFILES_DIR" status --porcelain)" ] || {
      printf 'bootstrap: dotfiles checkout has local changes\n' >&2
      exit 1
    }

    git -C "$DOTFILES_DIR" fetch --quiet origin master

    current_branch="$(git -C "$DOTFILES_DIR" symbolic-ref --quiet --short HEAD || true)"
    [ -z "$current_branch" ] || [ "$current_branch" = master ] || {
      printf 'bootstrap: expected dotfiles checkout on master or detached HEAD, got %s\n' \
        "$current_branch" >&2
      exit 1
    }

    git -C "$DOTFILES_DIR" merge-base --is-ancestor \
      HEAD refs/remotes/origin/master || {
      printf 'bootstrap: dotfiles checkout has commits outside origin/master\n' >&2
      exit 1
    }
  fi

  bootstrap_revision="$(git -C "$DOTFILES_DIR" rev-parse refs/remotes/origin/master)"
  git -C "$DOTFILES_DIR" checkout --quiet --detach "$bootstrap_revision"

  "$DOTFILES_DIR/bin/install-mise.sh"

  cd "$DOTFILES_DIR"
  "$MISE_BIN" trust
  "$MISE_BIN" install
  "$MISE_BIN" run system:apply
  "$MISE_BIN" run apply

  cd "$HOME"
  "$MISE_BIN" bootstrap --yes --verbose

  git -C "$DOTFILES_DIR" checkout --quiet -B master "$bootstrap_revision"
  git -C "$DOTFILES_DIR" branch --quiet --set-upstream-to=origin/master master
}

main "$@"
