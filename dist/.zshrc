# 環境変数
export LANG=ja_JP.UTF-8

: "zcompile" && {
  # zshファイル更新したら自動でコンパイル
  function() {for arg; do
    if [ ! -f "${HOME}/${arg}.zwc" -o "${HOME}/${arg}" -nt "${arg}.zwc" ]; then
      zcompile "${HOME}/${arg}"
    fi
  done} .zshrc .zshenv
}
: "zinit" && {
  ZINIT_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/zinit/zinit.git"
  if command -v zinit > /dev/null 2>&1; then
    # zinit installation
    # https://github.com/zdharma-continuum/zinit#manual
    mkdir -p "$(dirname $ZINIT_HOME)"
    git clone https://github.com/zdharma-continuum/zinit.git "$ZINIT_HOME"
  fi
  source "${ZINIT_HOME}/zinit.zsh"

  zinit light momo-lab/zsh-abbrev-alias # 略語を展開する
  # zinit ice wait'!0' lucid; zinit light zdharma/fast-syntax-highlighting
  zinit ice wait'!0' lucid; zinit light zsh-users/zsh-syntax-highlighting
}
: "iyashi" && {
  if [ $((${RANDOM} % 2)) = 0 ]; then
    nonnonbiyori
  else
    renchon
  fi
}
# ref: https://suin.io/568
: "general" && {
  setopt correct # コマンドのスペルを訂正
  setopt no_beep # ビープ音を鳴らさない
  setopt print_eight_bit # 日本語ファイル名を表示可能にする
  unsetopt promptcr # 改行のない出力をpromptで上書きするのを防ぐ
  bindkey "^[[3~" delete-char # delete key有効化
  # lsをカラー表示
  export LSCOLORS=exfxcxdxbxegedabagacad
  export LS_COLORS='di=34:ln=35:so=32:pi=33:ex=31:bd=46;34:cd=43;34:su=41;30:sg=46;30:tw=42;30:ow=43;30'
  zstyle ':completion:*' list-colors 'di=34' 'ln=35' 'so=32' 'ex=31' 'bd=46;34' 'cd=43;34'
}
: "history" && {
  HISTFILE="${HOME}"/.zsh_history
  HISTSIZE=10000
  SAVEHIST=10000
  setopt hist_ignore_dups # 直前のコマンドの重複を削除
  setopt hist_ignore_all_dups # 同じコマンドをヒストリに残さない
  setopt share_history # 同時に起動したzshの間でヒストリを共有
}

# prompt
[ -e "${HOME}/.zsh.local/prompt.zsh" ] && source "${HOME}/.zsh.local/prompt.zsh"

# alias
[ -e "${HOME}/alias.sh" ] && source "${HOME}/alias.sh"
[ -e "${HOME}/.zsh.local/alias.zsh" ] && source "${HOME}/.zsh.local/alias.zsh"

# keybindings
[ -e "${HOME}/.zsh.local/keybindings.zsh" ] && source "${HOME}/.zsh.local/keybindings.zsh"

# functions
[ -e "${HOME}/.zsh.local/functions.zsh" ] && source "${HOME}/.zsh.local/functions.zsh"

# note this assumes mise is located at ~/.local/bin/mise
# which is what install.sh does by default
export PATH="$HOME/.local/share/mise/shims:$PATH"

# The next line updates PATH for the Google Cloud SDK.
if [ -f "${HOME}/google-cloud-sdk/path.zsh.inc" ]; then . "${HOME}/google-cloud-sdk/path.zsh.inc"; fi

# The next line enables shell command completion for gcloud.
if [ -f "${HOME}/google-cloud-sdk/completion.zsh.inc" ]; then . "${HOME}/google-cloud-sdk/completion.zsh.inc"; fi
export PATH="/usr/local/opt/sphinx-doc/bin:$PATH"

# fin.
### End of Zinit's installer chunk
