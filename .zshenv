# $HOME/.zsh/.zshrcを読み込む
# export ZDOTDIR="$HOME"/.zsh

# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# Settings for anyenv
export PATH="$HOME/.anyenv/bin:$PATH"
#
# eval "$(anyenv init - --no-rehash)" # 遅い
#
source "$HOME/.anyenv/libexec/../completions/anyenv.zsh"
anyenv() {
  typeset command
  command="$1"
  if [ "$#" -gt 0 ]; then
    shift
  fi
  command anyenv "$command" "$@"
}
export GOENV_ROOT="$HOME/.anyenv/envs/goenv"
export PATH="$HOME/.anyenv/envs/goenv/bin:$PATH"
export PATH="$HOME/.anyenv/envs/goenv/shims:${PATH}"
export GOENV_SHELL=zsh
source "$HOME/.anyenv/envs/goenv/libexec/../completions/goenv.zsh"
goenv() {
  local command
  command="$1"
  if [ "$#" -gt 0 ]; then
    shift
  fi

  case "$command" in
  rehash|shell)
    eval "$(goenv "sh-$command" "$@")";;
  *)
    command goenv "$command" "$@";;
  esac
}
export HSENV_ROOT="$HOME/.anyenv/envs/hsenv"
export PATH="$HOME/.anyenv/envs/hsenv/bin:$PATH"
export PATH="$HOME/.anyenv/envs/hsenv/shims:${PATH}"
hsenv rehash 2>/dev/null
export PYENV_ROOT="$HOME/.anyenv/envs/pyenv"
export PATH="$HOME/.anyenv/envs/pyenv/bin:$PATH"
export PATH="$HOME/.anyenv/envs/pyenv/shims:${PATH}"
export PYENV_SHELL=zsh
source "$HOME/.anyenv/envs/pyenv/libexec/../completions/pyenv.zsh"
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
export RBENV_ROOT="$HOME/.anyenv/envs/rbenv"
export PATH="$HOME/.anyenv/envs/rbenv/bin:$PATH"
export PATH="$HOME/.anyenv/envs/rbenv/shims:${PATH}"
export RBENV_SHELL=zsh
source "$HOME/.anyenv/envs/rbenv/libexec/../completions/rbenv.zsh"
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
# anyenv ここまで
#


# Settings for Python
#export PYTHONPATH="/usr/local/lib/python3.7/site-packages:$PYTHONPATH"

# Settings for nodebrew
export PATH="$PATH:$HOME/.nodebrew/current/bin"

# PATH general
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"
