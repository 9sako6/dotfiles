# $HOME/.zsh/.zshrcを読み込む
export ZDOTDIR="$HOME"/.zsh
# /etc/profile を読み込まない設定
# 勝手に読み込まれるとPATH先頭に/usr/binが来てanyenvで入れた*envのPATHが読み込まれない
setopt no_global_rcs

# Settings for anyenv
export PATH="$HOME/.anyenv/bin:$PATH"
# eval "$(anyenv init - --no-rehash)" # 遅い
eval "$(env PATH="$ANYENV_ROOT/libexec:$PATH" $ANYENV_ROOT/libexec/anyenv-init - --no-rehash)"

# Settings for Python
#export PYTHONPATH="/usr/local/lib/python3.7/site-packages:$PYTHONPATH"

# Settings for nodebrew
export PATH="$PATH:$HOME/.nodebrew/current/bin"

# PATH general
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Library/TeX/texbin"

# Setting for original commands
export PATH="$PATH:$HOME/mybin"
