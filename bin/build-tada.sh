#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
source_path="$repo_root/lib/tada.swift"
artifact_path="$repo_root/home/mybin/lib/tada-darwin-arm64"
artifact_name=${artifact_path##*/}
expected_compiler="Apple Swift version 6.2.3 (swift-6.2.3-RELEASE)"
actual_compiler=$(swiftc --version 2>/dev/null | sed -n '1p')

case "$#" in
  0) check_only=false ;;
  1)
    if [ "$1" != "--check" ]; then
      printf 'usage: %s [--check]\n' "$0" >&2
      exit 2
    fi
    check_only=true
    ;;
  *)
    printf 'usage: %s [--check]\n' "$0" >&2
    exit 2
    ;;
esac

if [ "$actual_compiler" != "$expected_compiler" ]; then
  printf 'build-tada: expected %s, found %s\n' "$expected_compiler" "${actual_compiler:-no Swift compiler}" >&2
  exit 1
fi

sdk_path=$(xcrun --sdk macosx --show-sdk-path)
build_dir=$(mktemp -d "${TMPDIR:-/tmp}/tada-build.XXXXXX")
trap 'rm -rf "$build_dir"' EXIT HUP INT TERM
mkdir "$build_dir/module-cache"

CLANG_MODULE_CACHE_PATH="$build_dir/module-cache" swiftc \
  -sdk "$sdk_path" \
  -target arm64-apple-macosx15.0 \
  -O \
  -whole-module-optimization \
  -Xlinker -no_uuid \
  -o "$build_dir/$artifact_name" \
  "$source_path"

if [ "$check_only" = true ]; then
  if ! cmp -s "$build_dir/$artifact_name" "$artifact_path"; then
    printf 'build-tada: %s is not generated from %s\n' "$artifact_path" "$source_path" >&2
    exit 1
  fi
  exit 0
fi

install -m 0755 "$build_dir/$artifact_name" "$artifact_path"
