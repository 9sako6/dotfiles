source ~/.zshenv
source ~/.zshrc

# Test installed deps
if ! command -v fzf > /dev/null; then
  echo "Error: fail to install fzf." >&2
  exit 1
else
  fzf --version
fi

if ! command -v deno > /dev/null; then
  echo "Error: fail to install deno." >&2
  exit 1
else
  deno --version
fi
