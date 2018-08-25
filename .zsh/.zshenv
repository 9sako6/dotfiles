# $HOME/.zsh/.zshrcを読み込む
export ZDOTDIR=$HOME/.zsh
# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# anyenv
export PATH="$HOME/.anyenv/bin(N-/):$PATH"
eval "$(anyenv init -)"

# PATH general
export PATH="$PATH:/usr/local/bin(N-/):/usr/bin(N-/):/bin(N-/):/usr/sbin(N-/):/sbin(N-/):/Library/TeX/texbin(N-/):"
