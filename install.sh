# Install Deno
export DENO_INSTALL="$HOME/.local"
export PATH="$DENO_INSTALL/bin:$PATH"
curl -fsSL https://deno.land/x/install/install.sh | sh

deno run --allow-write --allow-read --allow-env main.ts
