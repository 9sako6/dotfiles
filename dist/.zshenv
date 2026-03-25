# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# PATH general
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin:$PATH"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"

# Set PATH, MANPATH, etc., for Homebrew.
[ -f '/opt/homebrew/bin/brew' ] && eval "$(/opt/homebrew/bin/brew shellenv)"
[ -f '/usr/local/bin/brew' ] && eval "$(/usr/local/bin/brew shellenv)"
[ -f '/home/linuxbrew/.linuxbrew/bin/brew' ] && eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"
