#!/bin/bash

source ~/.zshenv
source ~/.zshrc

check_dependency() {
  local cmd="$1"
  local version_cmd="$2"

  if ! command -v "$cmd" > /dev/null; then
    echo "Error: fail to install $cmd." >&2
    exit 1
  else
    echo "Success to install $cmd."
    eval "$version_cmd"
  fi
}

# Test installed deps
check_dependency "fzf" "fzf --version"
check_dependency "deno" "deno --version"
check_dependency "kubectl" "kubectl version --client"
check_dependency "minikube" "minikube version"
