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
  ### Added by zinit's installer
  source "${HOME}/.zinit/bin/zinit.zsh"
  autoload -Uz _zinit
  (( ${+_comps} )) && _comps[zinit]=_zinit
  ### End of zinit's installer chunk

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

# fin.
### End of Zinit's installer chunk
