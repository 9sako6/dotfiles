abbrev-alias cd-="cd -"

# Git
abbrev-alias gpl='git pull --ff-only'
abbrev-alias gps='git push'
abbrev-alias gpf='git push --force-with-lease'
abbrev-alias gad='git add'
abbrev-alias gbr='git branch'
abbrev-alias gch='git checkout'
abbrev-alias gcf='git checkout $(git branch | fzf)'
abbrev-alias gcl='git clone'
abbrev-alias gco='git commit'
abbrev-alias gcm='git commit -m'
abbrev-alias gca='git commit -a'
abbrev-alias gcam='git commit -am'
abbrev-alias gl='git log --decorate'
abbrev-alias glg='git log --oneline --decorate --graph --all'
abbrev-alias gst='git status'
abbrev-alias gme='git merge'
abbrev-alias grb='git rebase'
abbrev-alias gre='git restore'
abbrev-alias gsw='git switch'
abbrev-alias gdf='git diff'

# terraform
abbrev-alias tf="terraform"

# general
alias v="code"
alias gpp="g++"
alias cr='cd "$(ghq root)/$(ghq list | fzf)"'
alias '$'='command'

# Modern CLI replacements
abbrev-alias ls='eza'
abbrev-alias ll='eza -l --git'
abbrev-alias la='eza -la --git'
abbrev-alias lt='eza --tree --level=2'
abbrev-alias cat='bat --paging=never'
abbrev-alias catp='bat'
