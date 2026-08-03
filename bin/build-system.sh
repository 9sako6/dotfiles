#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(/usr/bin/dirname -- "$0")" && pwd)"
. "${script_dir}/../lib/install-system.sh"

install_system_build_main "$script_dir"
