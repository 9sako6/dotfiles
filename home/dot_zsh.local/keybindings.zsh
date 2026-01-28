# Ctrl-Y：上のディレクトリに移動できる
{
  function cd-up { zle push-line && LBUFFER='builtin cd ..' && zle accept-line }
  zle -N cd-up
  bindkey "^Y" cd-up
}

# Ctrl-W：パスの文字列などをスラッシュ単位でdeleteできる
{
  autoload -U select-word-style
  select-word-style bash
}

# Ctrl-F：fzfでディレクトリ履歴参照
{
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

# Ctrl-R：fzfでコマンド履歴参照
{
  # source: https://tech-blog.sgr-ksmt.org/2016/12/10/smart_fzf_history/
  function select-history() {
    BUFFER=$(history -n -r 1 | fzf --no-sort +m --query "$LBUFFER" --prompt="History > ")
    CURSOR=$#BUFFER
  }
  zle -N select-history
  bindkey '^r' select-history
}
