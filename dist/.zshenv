# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# local bin
export PATH="/usr/local/bin:${PATH}"

# Settings for rbenv
export RBENV_ROOT="$HOME/.rbenv"
if [ -d "$RBENV_ROOT" ]; then
  export PATH="$RBENV_ROOT/bin:$PATH"
  if [ -d "${HOME}/.rbenv/bin/rbenv" ]; then
    eval "$(~/.rbenv/bin/rbenv init - zsh)"
  else
    eval "$(rbenv init - --no-rehash)"
  fi
fi

# Settings for nodebrew
export PATH="$PATH:$HOME/.nodebrew/current/bin"

# PATH general
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"

# Setting for fzf
[ -f ~/.fzf.zsh ] && source ~/.fzf.zsh

# Setting for nvm
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"  # This loads nvm bash_completion

# Rust
export PATH="$HOME/.cargo/bin:$PATH"

# Cargo
. "$HOME/.cargo/env"

# go
export GOPATH="$HOME/go"
export PATH="$GOPATH/bin:$PATH"
export PATH="$PATH:/usr/local/go/bin"

# goenv
export GOENV_ROOT="$HOME/.goenv"
export PATH="$GOENV_ROOT/bin:$PATH"
if type goenv > /dev/null 2>&1; then
  eval "$(goenv init -)"
  export PATH="$PATH:$GOPATH/bin"
fi

# deno
export DENO_INSTALL="$HOME/.local"
export PATH="$DENO_INSTALL/bin:$PATH"

# gnu-getopt
export PATH="/usr/local/opt/gnu-getopt/bin:$PATH"

# opam configuration
test -r /Users/9sako6/.opam/opam-init/init.zsh && . /Users/9sako6/.opam/opam-init/init.zsh > /dev/null 2> /dev/null || true

# depot_tools for Chromium
# https://chromium.googlesource.com/chromium/src/+/main/docs/mac_build_instructions.md#install
export PATH="$PATH:$HOME/ghq/chromium.googlesource.com/chromium/tools/depot_tools"

# protobuf
export PATH="$PATH:/usr/local/protobuf/bin"

# K8s auto-complete
autoload -U +X compinit && compinit
source <(kubectl completion zsh)

# Set PATH, MANPATH, etc., for Homebrew.
[ -f '/home/linuxbrew/.linuxbrew/bin/brew' ] && eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"

# minikube
eval $(minikube -p minikube docker-env)

# aqua
export PATH="${AQUA_ROOT_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/aquaproj-aqua}/bin:$PATH"
export AQUA_GLOBAL_CONFIG=${AQUA_GLOBAL_CONFIG:-}:${XDG_CONFIG_HOME:-$HOME/.config}/aquaproj-aqua/aqua.yaml

# asdf
. "$HOME/.asdf/asdf.sh"
. "$HOME/.asdf/completions/asdf.bash"
