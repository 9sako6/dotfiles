[[ -o interactive ]] || return

: "zcompile" && {
  # zshファイル更新したら自動でコンパイル
  function() {for arg; do
    if [ ! -f "${HOME}/${arg}.zwc" -o "${HOME}/${arg}" -nt "${arg}.zwc" ]; then
      zcompile "${HOME}/${arg}"
    fi
  done} .zshrc .zshenv
}

# ref: https://suin.io/568
: "general" && {
  autoload -U +X compinit && compinit
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

# note this assumes mise is located at ~/.local/bin/mise
# which is what install.sh does by default
eval "$(~/.local/bin/mise activate zsh)"
export PATH="$HOME/.local/share/mise/shims:$PATH"

: "zinit" && {
  ZINIT_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/zinit/zinit.git"
  if [ -f "${ZINIT_HOME}/zinit.zsh" ]; then
    source "${ZINIT_HOME}/zinit.zsh"

    zinit light momo-lab/zsh-abbrev-alias # 略語を展開する
    zinit ice wait'!0' lucid; zinit light zsh-users/zsh-syntax-highlighting
  fi
}

# prompt
[ -e "${HOME}/.zsh.d/prompt.zsh" ] && source "${HOME}/.zsh.d/prompt.zsh"

# alias
[ -e "${HOME}/alias.sh" ] && source "${HOME}/alias.sh"
[ -e "${HOME}/.zsh.d/alias.zsh" ] && source "${HOME}/.zsh.d/alias.zsh"

# keybindings
[ -e "${HOME}/.zsh.d/keybindings.zsh" ] && source "${HOME}/.zsh.d/keybindings.zsh"

# functions
[ -e "${HOME}/.zsh.d/functions.zsh" ] && source "${HOME}/.zsh.d/functions.zsh"

[ -e "${HOME}/.zsh.d/local.zsh" ] && source "${HOME}/.zsh.d/local.zsh"
[ -e "${HOME}/.zsh.d/secrets.zsh" ] && source "${HOME}/.zsh.d/secrets.zsh"

: "iyashi" && {
  if [ -z "${DOTFILES_NO_BANNER:-}" ] && [ -z "${CI:-}" ]; then
    if [ $((${RANDOM} % 2)) = 0 ]; then
      nonnonbiyori
    else
      renchon
    fi
  fi
}

if mise which direnv > /dev/null 2>&1; then
  eval "$(direnv hook zsh)"
fi
