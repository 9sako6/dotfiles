#!/bin/sh
set -eu

# Migration helper for environments where dotfiles are symlinked from dist/.
# It backs up current files, fixes symlinks, then applies the repo's home/ via chezmoi.

REPO=${REPO:-"9sako6/dotfiles"}
# SOURCE_DIR should point to the directory that contains .zshrc (i.e. .../home).
SOURCE_DIR=${SOURCE_DIR:-}
BACKUP_DIR=${BACKUP_DIR:-"$HOME/.dotfiles.backup-$(date +%Y%m%d)"}
APPLY_FLAGS=${APPLY_FLAGS:-"--force"}

FILES="
$HOME/.zshrc
$HOME/.zshenv
$HOME/.gitconfig
$HOME/.gitignore_global
$HOME/.zoirc.json
$HOME/alias.sh
"

DIRS="
$HOME/.zsh.local
$HOME/mybin
"

say() {
  printf '%s\n' "$*"
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    say "missing command: $1" >&2
    exit 1
  fi
}

ensure_chezmoi() {
  if command -v chezmoi >/dev/null 2>&1; then
    return
  fi
  need_cmd curl
  mkdir -p "$HOME/.local/bin"
  export PATH="$HOME/.local/bin:$PATH"
  sh -c "$(curl -fsLS get.chezmoi.io)"
  if ! command -v chezmoi >/dev/null 2>&1; then
    say "failed to install chezmoi" >&2
    exit 1
  fi
}

backup_files() {
  say "backup -> $BACKUP_DIR"
  mkdir -p "$BACKUP_DIR"
  if command -v rsync >/dev/null 2>&1; then
    rsync -aL $FILES $DIRS "$BACKUP_DIR"/ 2>/dev/null || true
  else
    # best-effort fallback (follows symlinks with -L if available)
    for path in $FILES $DIRS; do
      if [ -e "$path" ]; then
        cp -aL "$path" "$BACKUP_DIR"/ 2>/dev/null || cp -a "$path" "$BACKUP_DIR"/
      fi
    done
  fi
}

resolve_source_dir() {
  if [ -n "$SOURCE_DIR" ]; then
    if [ -d "$SOURCE_DIR/home" ] && [ ! -f "$SOURCE_DIR/.zshrc" ]; then
      SOURCE_DIR="$SOURCE_DIR/home"
    fi
    if [ ! -f "$SOURCE_DIR/.zshrc" ]; then
      say "SOURCE_DIR does not contain dotfiles: $SOURCE_DIR" >&2
      exit 1
    fi
    return
  fi

  if [ -d "$HOME/dotfiles/home" ]; then
    SOURCE_DIR="$HOME/dotfiles/home"
    return
  fi

  if command -v git >/dev/null 2>&1; then
    say "init from repo: $REPO"
    chezmoi init "$REPO"
    SOURCE_DIR="$(chezmoi source-path)/home"
    if [ ! -d "$SOURCE_DIR" ]; then
      say "home/ not found in chezmoi source: $SOURCE_DIR" >&2
      exit 1
    fi
    return
  fi

  need_cmd tar
  say "git not found; download archive for $REPO"
  tmpdir="$(mktemp -d)"
  cleanup() {
    rm -rf "$tmpdir"
  }
  trap cleanup EXIT

  archive_url="https://github.com/${REPO}/archive/refs/heads/master.tar.gz"
  curl -fsSL "$archive_url" -o "$tmpdir/repo.tar.gz"
  tar -xzf "$tmpdir/repo.tar.gz" -C "$tmpdir"
  extracted="$(find "$tmpdir" -maxdepth 1 -type d -name 'dotfiles-*' -print | head -n 1)"
  if [ -z "$extracted" ]; then
    say "failed to extract repository archive." >&2
    exit 1
  fi
  source_root="$HOME/.local/share/chezmoi"
  mkdir -p "$source_root"
  cp -a "$extracted/." "$source_root/"
  SOURCE_DIR="$source_root/home"
  if [ ! -d "$SOURCE_DIR" ]; then
    say "home/ not found in extracted repo." >&2
    exit 1
  fi
}

remove_symlinks() {
  for path in $FILES $DIRS; do
    if [ -L "$path" ]; then
      rm "$path"
    fi
  done

  for dir in $DIRS; do
    if [ -d "$dir" ]; then
      find "$dir" -type l -delete 2>/dev/null || true
    fi
  done
}

apply_chezmoi() {
  if [ -z "$SOURCE_DIR" ]; then
    say "SOURCE_DIR is empty" >&2
    exit 1
  fi
  say "apply from source: $SOURCE_DIR"
  # Use -S to ensure the repo's home/ is used.
  # APPLY_FLAGS can be overridden, default is --force.
  chezmoi -S "$SOURCE_DIR" apply $APPLY_FLAGS
}

fix_mybin_permissions() {
  if [ -d "$HOME/mybin" ]; then
    find "$HOME/mybin" -type f -exec chmod +x {} \;
  fi
}

say "start migrate -> chezmoi"
ensure_chezmoi
backup_files
resolve_source_dir
remove_symlinks
apply_chezmoi
fix_mybin_permissions
say "done. run: exec zsh"
