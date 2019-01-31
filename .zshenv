# $HOME/.zsh/.zshrcを読み込む
# export ZDOTDIR="$HOME"/.zsh

# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# Settings for rbenv
export RBENV_ROOT="$HOME/.rbenv"
if [ -d "$RBENV_ROOT" ]; then
  export PATH="$RBENV_ROOT/bin:$PATH"
  #
  # eval "$(rbenv init -)"
  #
  export PATH="$HOME/.rbenv/shims:${PATH}"
  export RBENV_SHELL=zsh
  source "$HOME/.rbenv/libexec/../completions/rbenv.zsh"
  command rbenv rehash 2>/dev/null
  rbenv() {
    local command
    command="${1:-}"
    if [ "$#" -gt 0 ]; then
      shift
    fi

    case "$command" in
    rehash|shell)
      eval "$(rbenv "sh-$command" "$@")";;
    *)
      command rbenv "$command" "$@";;
    esac
  }
  #
  #
  #
fi

# Settings for pyenv
export PYENV_ROOT="$HOME/.pyenv"
if [ -d "$PYENV_ROOT" ]; then
  export PATH="$PYENV_ROOT/bin:$PATH"
  #
  # eval "$(pyenv init -)"
  #
  export PATH="$HOME/.pyenv/shims:${PATH}"
  export PYENV_SHELL=zsh
  source "$HOME/.pyenv/libexec/../completions/pyenv.zsh"
  command pyenv rehash 2>/dev/null
  pyenv() {
    local command
    command="${1:-}"
    if [ "$#" -gt 0 ]; then
      shift
    fi

    case "$command" in
      activate|deactivate|rehash|shell)
      eval "$(pyenv "sh-$command" "$@")";;
      *)
      command pyenv "$command" "$@";;
    esac
  }
  #
  #
  #
fi

# Settings for Python
#export PYTHONPATH="/usr/local/lib/python3.7/site-packages:$PYTHONPATH"

# Settings for nodebrew
export PATH="$PATH:$HOME/.nodebrew/current/bin"

# PATH general
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"
