# Show k8s current context
zinit light jonmosco/kube-ps1
KUBE_PS1_CTX_COLOR='magenta'
KUBE_PS1_NS_COLOR='white'
KUBE_PS1_PREFIX=''
KUBE_PS1_SUFFIX=''
KUBE_PS1_SEPARATOR=' '

autoload -Uz colors
colors
autoload -Uz vcs_info
setopt prompt_subst
zstyle ':vcs_info:git:*' check-for-changes true
zstyle ':vcs_info:git:*' stagedstr "%F{yellow}!"
zstyle ':vcs_info:git:*' unstagedstr "%F{red}+"
zstyle ':vcs_info:*' formats "%F{green}%c%u[%b]%f"
zstyle ':vcs_info:*' actionformats '[%b|%a]'
precmd () { vcs_info }
RPROMPT=$RPROMPT'%F{cyan}。(｡>﹏<｡)'
PROMPT="%F{cyan}%n@%d \${vcs_info_msg_0_} \$(kube_ps1)
%F{cyan}( ;ᴗ;)っ%f "
