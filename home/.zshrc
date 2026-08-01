[[ -o interactive ]] || return

: "zcompile" && {
  # zshファイル更新したら自動でコンパイル
  function() {for arg; do
    if [ ! -f "${HOME}/${arg}.zwc" -o "${HOME}/${arg}" -nt "${HOME}/${arg}.zwc" ]; then
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

if mise which zoxide > /dev/null 2>&1; then
  eval "$(zoxide init zsh)"
fi

if mise which atuin > /dev/null 2>&1; then
  export ATUIN_NOBIND="true"
  eval "$(atuin init zsh)"
fi

: "zinit" && {
  ZINIT_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/zinit/zinit.git"
  if [ -f "${ZINIT_HOME}/zinit.zsh" ]; then
    source "${ZINIT_HOME}/zinit.zsh"

    zinit_light_pinned() {
      local repository="$1"
      local revision="$2"
      local plugin_dir="${ZINIT[PLUGINS_DIR]}/${repository//\//---}"

      if [ ! -d "$plugin_dir/.git" ]; then
        command git clone --no-checkout "https://github.com/${repository}.git" "$plugin_dir" || return 1
      fi

      command git -C "$plugin_dir" checkout --quiet --detach "$revision" || {
        command git -C "$plugin_dir" fetch --depth 1 origin "$revision" &&
          command git -C "$plugin_dir" checkout --quiet --detach "$revision"
      } || return 1
      command git -C "$plugin_dir" clean --force --quiet -- '*.zwc' || return 1

      local resolved_revision="$(command git -C "$plugin_dir" rev-parse HEAD)"
      if [[ "$resolved_revision" != "$revision" || -n "$(command git -C "$plugin_dir" status --porcelain)" ]]; then
        print -u2 "zinit: refusing ${repository} at ${resolved_revision:-unknown}; expected ${revision}"
        return 1
      fi

      zinit ice nocompile
      zinit light "$repository"
    }

    zinit_light_pinned momo-lab/zsh-abbrev-alias 33fe094da0a70e279e1cc5376a3d7cb7a5343df5
    zinit_light_pinned zsh-users/zsh-syntax-highlighting 1d85c692615a25fe2293bdd44b34c217d5d2bf04
    zinit_light_pinned zsh-users/zsh-autosuggestions 85919cd1ffa7d2d5412f6d3fe437ebdbeeec4fc5
    unfunction zinit_light_pinned
  fi
}

# prompt
[ -e "${HOME}/.zsh.d/prompt.zsh" ] && source "${HOME}/.zsh.d/prompt.zsh"

# alias
[ -e "${HOME}/.zsh.d/alias.zsh" ] && source "${HOME}/.zsh.d/alias.zsh"

# keybindings
[ -e "${HOME}/.zsh.d/keybindings.zsh" ] && source "${HOME}/.zsh.d/keybindings.zsh"

# functions
[ -e "${HOME}/.zsh.d/functions.zsh" ] && source "${HOME}/.zsh.d/functions.zsh"

[ -e "${HOME}/.zsh.d/local.zsh" ] && source "${HOME}/.zsh.d/local.zsh"

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
