# /etc/profile を読み込まない設定
setopt no_global_rcs

export LANG=ja_JP.UTF-8
export GHQ_ROOT="${GHQ_ROOT:-$HOME/ghq}"

typeset -U path PATH
path=(
  "$HOME/.nix-profile/bin"
  /run/current-system/sw/bin
  "$HOME/.local/bin"
  /usr/local/bin
  /usr/bin
  /bin
  /usr/sbin
  /sbin
  /Library/TeX/texbin
  $path
  "$HOME/mybin"
)
export PATH

export SSH_AUTH_SOCK="${SSH_AUTH_SOCK:-$HOME/.ssh/agent.sock}"

if [[ -r "${NIX_SSL_CERT_FILE:-/etc/ssl/cert.pem}" ]]; then
  export SSL_CERT_FILE="${SSL_CERT_FILE:-${NIX_SSL_CERT_FILE:-/etc/ssl/cert.pem}}"
  export PIP_CERT="${PIP_CERT:-$SSL_CERT_FILE}"
  export REQUESTS_CA_BUNDLE="${REQUESTS_CA_BUNDLE:-$SSL_CERT_FILE}"
  export AWS_CA_BUNDLE="${AWS_CA_BUNDLE:-$SSL_CERT_FILE}"
fi

# Set secret environment variables
[ -e "${HOME}/.zsh.d/secrets.zsh" ] && source "${HOME}/.zsh.d/secrets.zsh"
