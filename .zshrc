# 環境変数
export LANG=ja_JP.UTF-8

# ref: https://suin.io/568
: "general" && {
  setopt correct # コマンドのスペルを訂正
  setopt no_beep # ビープ音を鳴らさない
  setopt print_eight_bit # 日本語ファイル名を表示可能にする
  unsetopt promptcr # 改行のない出力をpromptで上書きするのを防ぐ
}
: "completion" && {
  autoload -Uz compinit && compinit -u # 補完機能強化
  # zstyle ':completion:*' matcher-list 'm:{a-z}={A-Z}' # 補完で小文字でも大文字にマッチさせる
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
  : "Ctrl-[で直前コマンドの単語を挿入できる" && {
    autoload -Uz smart-insert-last-word
    zstyle :insert-last-word match '*([[:alpha:]/\\]?|?[[:alpha:]/\\])*' # [a-zA-Z], /, \ のうち少なくとも1文字を含む長さ2以上の単語
    zle -N insert-last-word smart-insert-last-word
    bindkey '^[' insert-last-word
    # see http://qiita.com/mollifier/items/1a9126b2200bcbaf515f
  }
}

: "prompt" && {
  autoload -Uz colors
  colors
  PROMPT="%F{cyan}[%d]
$%f "
}
