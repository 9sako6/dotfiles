autoload -Uz colors
colors
autoload -Uz vcs_info
setopt prompt_subst
zstyle ':vcs_info:git:*' check-for-changes true
zstyle ':vcs_info:git:*' stagedstr "%F{yellow}!"
zstyle ':vcs_info:git:*' unstagedstr "%F{red}+"
zstyle ':vcs_info:*' formats "%F{green}%c%u[%b]%f"
zstyle ':vcs_info:*' actionformats '[%b|%a]'
zstyle ':vcs_info:*+set-message:*' hooks escape-prompt
+vi-escape-prompt() {
  hook_com[branch]=${hook_com[branch]//'%'/%%}
  hook_com[action]=${hook_com[action]//'%'/%%}
}
precmd () { vcs_info }
PROMPT="%F{cyan}%n@%d \${vcs_info_msg_0_}
%F{cyan}( ;ᴗ;)っ%f "
