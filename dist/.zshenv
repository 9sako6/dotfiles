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

# PATH general
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"

# Setting for nvm
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"  # This loads nvm bash_completion

# gnu-getopt
export PATH="/usr/local/opt/gnu-getopt/bin:$PATH"

# opam configuration
test -r /Users/9sako6/.opam/opam-init/init.zsh && . /Users/9sako6/.opam/opam-init/init.zsh > /dev/null 2> /dev/null || true

# K8s auto-complete
autoload -U +X compinit && compinit
if command -v kubectl > /dev/null 2>&1; then
  source <(kubectl completion zsh)
fi

# Set PATH, MANPATH, etc., for Homebrew.
[ -f '/home/linuxbrew/.linuxbrew/bin/brew' ] && eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"

# krew (kubernetes plugin manager)
export PATH="${KREW_ROOT:-$HOME/.krew}/bin:$PATH"
