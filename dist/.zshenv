# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

export LANG=ja_JP.UTF-8

# PATH general
export PATH="/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin:$PATH"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"

# Set secret environment variables
[ -e "${HOME}/.zsh.d/secrets.zsh" ] && source "${HOME}/.zsh.d/secrets.zsh"
