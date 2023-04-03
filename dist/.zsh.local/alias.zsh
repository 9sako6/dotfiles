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

# Docker
abbrev-alias dc="docker-compose"

# Electron
abbrev-alias electron="~/node_modules/.bin/electron"

# zoi
abbrev-alias zoit="date "+%Y%m%d.md" | EDITOR=code zoi open"
abbrev-alias zoic='cd $(zoi list -d | fzf)'

# InteliJ
if [ -d "${HOME}/idea-IU-223.7571.182" ]; then
  abbrev-alias idea="~/idea-IU-223.7571.182/bin/idea.sh"
else
  abbrev-alias idea="intellij-idea-community"
fi

# Kubernetes
abbrev-alias k="kubectl"

# cargo
abbrev-alias crun='RUST_LOG=DEBUG cargo -vv watch -x run'
