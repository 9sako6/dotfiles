import { applyCopies } from "../lib/home-copy.ts";
import { home, repository, run } from "../lib/process.ts";

await run(["mise", "dotfiles", "apply", "--yes"]);
await applyCopies(repository, home!, "home");
await run(["mise", "install", "--locked"], repository, {
  ...process.env,
  MISE_CONFIG_FILE: `${repository}/home/.config/mise/config.toml`,
});
await run(["mise", "bootstrap", "repos", "apply", "--yes"], repository, {
  ...process.env,
  MISE_CONFIG_FILE: `${repository}/home/.config/mise/config.toml`,
});
