#!/bin/sh
set -eu

main() {
  DOTFILES_DIR="${DOTFILES_DIR:-$HOME/dotfiles}"
  DOTFILES_REPO_URL="${DOTFILES_REPO_URL:-https://github.com/9sako6/dotfiles.git}"
  DOTFILES_REVISION="${DOTFILES_REVISION:-f193a5a832ffffbd772135b72527418067d0aa5c}"
  MISE_BIN="${HOME}/.local/bin/mise"
  new_checkout=0

  if [ ! -d "$DOTFILES_DIR/.git" ]; then
    git clone --no-checkout "$DOTFILES_REPO_URL" "$DOTFILES_DIR"
    git -C "$DOTFILES_DIR" checkout --detach "$DOTFILES_REVISION"
    new_checkout=1
  fi

  resolved_revision="$(git -C "$DOTFILES_DIR" rev-parse HEAD)"
  [ "$resolved_revision" = "$DOTFILES_REVISION" ] || {
    printf 'bootstrap: expected dotfiles revision %s, got %s\n' \
      "$DOTFILES_REVISION" "$resolved_revision" >&2
    exit 1
  }

  [ -z "$(git -C "$DOTFILES_DIR" status --porcelain)" ] || {
    printf 'bootstrap: dotfiles checkout has local changes\n' >&2
    exit 1
  }

  "$DOTFILES_DIR/bin/install-mise.sh"

  cd "$DOTFILES_DIR"
  "$MISE_BIN" trust
  "$MISE_BIN" bootstrap --yes

  if [ "$new_checkout" = 1 ]; then
    git -C "$DOTFILES_DIR" checkout --quiet -B master "$DOTFILES_REVISION"
    git -C "$DOTFILES_DIR" branch --quiet --set-upstream-to=origin/master master
  fi
}

main "$@"
