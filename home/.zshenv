# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

export LANG=ja_JP.UTF-8

typeset -U path PATH
path=(
  /run/current-system/sw/bin
  "$HOME/.local/share/mise/shims"
  "$HOME/.local/bin"
  /usr/local/bin
  /usr/bin
  /bin
  /usr/sbin
  /sbin
  /Library/TeX/texbin
  $path
  "$HOME/mybin"
)
export PATH

# Set secret environment variables
[ -e "${HOME}/.zsh.d/secrets.zsh" ] && source "${HOME}/.zsh.d/secrets.zsh"
