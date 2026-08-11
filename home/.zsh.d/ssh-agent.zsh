# Managed by dotfiles — fixed ssh-agent socket for nix-daemon SSH fetch
if [ -z "${SSH_AUTH_SOCK:-}" ] || [ ! -S "${SSH_AUTH_SOCK:-}" ]; then
  _dotfiles_ssh_sock="${HOME}/.ssh/agent.sock"
  if [ -S "$_dotfiles_ssh_sock" ]; then
    export SSH_AUTH_SOCK="$_dotfiles_ssh_sock"
  else
    _dotfiles_ssh_agent_out="$(ssh-agent -a "$_dotfiles_ssh_sock" -s 2>/dev/null)" || true
    if [ -S "$_dotfiles_ssh_sock" ]; then
      eval "$_dotfiles_ssh_agent_out" >/dev/null 2>&1 || true
      export SSH_AUTH_SOCK="$_dotfiles_ssh_sock"
      # Do not auto-add keys; user runs `ssh-add` as needed.
      # Hint once per shell if no identities are loaded.
      if ! ssh-add -l >/dev/null 2>&1; then
        case "$-" in *i*) printf 'ssh-agent: no identities loaded. run: ssh-add ~/.ssh/id_ed25519\n' >&2 ;; esac
      fi
    else
      case "$-" in *i*) printf 'dotfiles: could not start ssh-agent at %s\n' "$_dotfiles_ssh_sock" >&2 ;; esac
    fi
    unset _dotfiles_ssh_agent_out
  fi
  unset _dotfiles_ssh_sock
else
  # SSH_AUTH_SOCK is already a live socket (e.g. forwarded or launchd); keep it.
  :
fi
