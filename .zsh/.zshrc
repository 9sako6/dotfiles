# 環境変数
export LANG=ja_JP.UTF-8

: "zplug" && {
  source ~/.zplug/init.zsh
  # (1) プラグインを定義する
  zplug 'momo-lab/zsh-abbrev-alias' # 略語を展開する
  zplug 'zsh-users/zsh-syntax-highlighting' # 実行可能なコマンドに色付け
  # (2) インストールする
  if ! zplug check --verbose; then
    printf 'Install? [y/N]: '
    if read -q; then
      echo; zplug install
    fi
  fi
  zplug load --verbose
}
# ref: https://suin.io/568
: "general" && {
  setopt correct # コマンドのスペルを訂正
  setopt no_beep # ビープ音を鳴らさない
  setopt print_eight_bit # 日本語ファイル名を表示可能にする
  unsetopt promptcr # 改行のない出力をpromptで上書きするのを防ぐ
  # lsをカラー表示
  export LSCOLORS=exfxcxdxbxegedabagacad
  export LS_COLORS='di=34:ln=35:so=32:pi=33:ex=31:bd=46;34:cd=43;34:su=41;30:sg=46;30:tw=42;30:ow=43;30'
  zstyle ':completion:*' list-colors 'di=34' 'ln=35' 'so=32' 'ex=31' 'bd=46;34' 'cd=43;34'
}
: "completion" && {
  autoload -Uz compinit && compinit -u # 補完機能強化
  setopt list_packed # 補完候補を詰めて表示
  zstyle ':completion:*' list-colors '' # 補完候補一覧をカラー表示
}
: "history" && {
  HISTFILE=$HOME/.zsh_history
  HISTSIZE=10000
  SAVEHIST=10000
  setopt hist_ignore_dups # 直前のコマンドの重複を削除
  setopt hist_ignore_all_dups # 同じコマンドをヒストリに残さない
  setopt share_history # 同時に起動したzshの間でヒストリを共有
}
: "key-bindings" && {
  : "Ctrl-Yで上のディレクトリに移動できる" && {
    function cd-up { zle push-line && LBUFFER='builtin cd ..' && zle accept-line }
    zle -N cd-up
    bindkey "^Y" cd-up
  }
  : "Ctrl-Wでパスの文字列などをスラッシュ単位でdeleteできる" && {
    autoload -U select-word-style
    select-word-style bash
  }
}
: "prompt" && {
  autoload -Uz colors
  colors
  PROMPT="%F{cyan}[%d]
$%f "
}
: "alias" && {
  alias tree="tree -NC" # N: 文字化け対策, C:色をつける
  abbrev-alias ls="ls -G"
  : "git" && {
    abbrev-alias gpl="git pull"
    abbrev-alias gps="git push"
    abbrev-alias gco="git commit -m"
    abbrev-alias gad="git add"
    abbrev-alias gbr="git branch"
    abbrev-alias gcl="git clone"
  }
}
# fin.
