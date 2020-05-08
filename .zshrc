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
# : "zplugin" && {
#   ### Added by Zplugin's installer
#   source "${HOME}/.zplugin/bin/zplugin.zsh"
#   autoload -Uz _zplugin
#   (( ${+_comps} )) && _comps[zplugin]=_zplugin
#   ### End of Zplugin's installer chunk

#   zplugin light momo-lab/zsh-abbrev-alias # 略語を展開する
#   zplugin ice wait'!0' lucid; zplugin light zdharma/fast-syntax-highlighting
# }
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
# Unnecessary, maybe
# source1: https://qiita.com/vintersnow/items/7343b9bf60ea468a4180#compinit
# source2: https://github.com/zplug/zplug/issues/24
# : "completion" && {
#   autoload -Uz compinit && compinit -u # 補完機能強化
#   setopt list_packed # 補完候補を詰めて表示
#   zstyle ':completion:*' list-colors '' # 補完候補一覧をカラー表示
# }
: "history" && {
  HISTFILE="${HOME}"/.zsh_history
  HISTSIZE=10000
  SAVEHIST=10000
  setopt hist_ignore_dups # 直前のコマンドの重複を削除
  setopt hist_ignore_all_dups # 同じコマンドをヒストリに残さない
  setopt share_history # 同時に起動したzshの間でヒストリを共有
}
: "key-bindings" && {
  : "Ctrl-Y：上のディレクトリに移動できる" && {
    function cd-up { zle push-line && LBUFFER='builtin cd ..' && zle accept-line }
    zle -N cd-up
    bindkey "^Y" cd-up
  }
  : "Ctrl-W：パスの文字列などをスラッシュ単位でdeleteできる" && {
    autoload -U select-word-style
    select-word-style bash
  }
  : "Ctrl-F：fzfでディレクトリ履歴参照" && {
    # https://rasukarusan.hatenablog.com/entry/2018/08/14/083000
    autoload -Uz chpwd_recent_dirs cdr add-zsh-hook
    add-zsh-hook chpwd chpwd_recent_dirs
    zstyle ':chpwd:*'      recent-dirs-max 100
    zstyle ':chpwd:*'      recent-dirs-default yes
    zstyle ':completion:*' recent-dirs-insert botho
    function fzf-cdr() {
      target_dir=`cdr -l | fzf | sed 's/^[^ ][^ ]*  *//'`
      target_dir=`echo ${target_dir/\~/${HOME}}`
      if [ -n "$target_dir" ]; then
        cd $target_dir
        BUFFER=""
        zle accept-line
      fi
    }
    zle -N fzf-cdr
    bindkey '^F' fzf-cdr
  }
  : "Ctrl-R：fzfでコマンド履歴参照" && {
    # https://tech-blog.sgr-ksmt.org/2016/12/10/smart_fzf_history/
    function select-history() {
      BUFFER=$(history -n -r 1 | fzf --no-sort +m --query "$LBUFFER" --prompt="History > ")
      CURSOR=$#BUFFER
    }
    zle -N select-history
    bindkey '^r' select-history
  }
}
: "prompt" && {
  autoload -Uz colors
  colors
  PROMPT="%F{cyan}%n@%d
( ;ᴗ;)っ%f "
  autoload -Uz vcs_info
  setopt prompt_subst
  zstyle ':vcs_info:git:*' check-for-changes true
  zstyle ':vcs_info:git:*' stagedstr "%F{yellow}!"
  zstyle ':vcs_info:git:*' unstagedstr "%F{red}+"
  zstyle ':vcs_info:*' formats "%F{green}%c%u[%b]%f"
  zstyle ':vcs_info:*' actionformats '[%b|%a]'
  precmd () { vcs_info }
  RPROMPT=$RPROMPT'${vcs_info_msg_0_}'
}
: "alias" && {
  alias tree="tree -NC" # N: 文字化け対策, C:色をつける
  alias v="code"
  alias gpp="g++"
  abbrev-alias ls="ls -G"
  abbrev-alias cd-="cd -"
  # if [ -e /usr/local/bin/vim ]; then
  #   alias vim="/usr/local/bin/vim"
  # fi
  : "git" && {
    abbrev-alias gpl="git pull"
    abbrev-alias gps="git push"
    abbrev-alias gad="git add"
    abbrev-alias gbr="git branch"
    abbrev-alias gch="git checkout"
    abbrev-alias gcl="git clone"
    abbrev-alias gco="git commit"
    abbrev-alias gcm="git commit -m"
    abbrev-alias glog="git log --decorate=full"
    abbrev-alias gst="git status"
    abbrev-alias gme="git merge"
    abbrev-alias grb="git rebase"
    abbrev-alias gre="git restore"
    abbrev-alias gsw="git switch"
  }
  : "docker-compose" && {
    abbrev-alias dc="docker-compose"
  }
  # electron
  abbrev-alias electron="~/node_modules/.bin/electron"
}

# neovim
export XDG_CONFIG_HOME="${HOME}/.config"


#
# commands
#
mkcd () {
  mkdir "${@}" && cd "${@}"
}

change_branch() {
  if type git switch >/dev/null 2>&1; then
    git switch $(git branch | fzf)
  else
    git checkout $(git branch | fzf)
  fi
}
alias cb=change_branch

# fin.
### End of Zinit's installer chunk
