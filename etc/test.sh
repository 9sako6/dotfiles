#!/bin/bash
# set -e

function command_exists() {
  command=${1}

  if type ${command}; then
    :
  else
    exit 1
  fi
}

make help
make init
make clean
make deploy

# NOTE: fail in github actions
# command_exists fzf
