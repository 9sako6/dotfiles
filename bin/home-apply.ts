import { applyCopies } from "../lib/home-copy.ts";
import path from "node:path";
import { capture, home, repository, run } from "../lib/process.ts";
import { retireNightlightLink } from "../lib/system-resources.ts";

const toolEnvironment = { ...process.env, MISE_CONFIG_FILE: `${repository}/home/.config/mise/config.toml`, UV_NO_BUILD: "1" };

await run(["mise", "dotfiles", "apply", "--yes"]);
await applyCopies(repository, home!, "home");
await run(["mise", "install", "--locked"], repository, toolEnvironment);
await capture(["mise", "which", "nightlight"], repository, toolEnvironment);
await retireNightlightLink(home!, path.join(process.env.XDG_STATE_HOME ?? path.join(home!, ".local/state"), "dotfiles"));
await run(["mise", "bootstrap", "repos", "apply", "--yes"], repository, toolEnvironment);
