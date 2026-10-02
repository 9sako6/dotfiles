import path from "node:path";
import { applyCopies } from "../lib/home-copy.ts";
import { home, repository, run } from "../lib/process.ts";

const operation = process.argv[2] ?? "apply";
const args = process.argv.slice(3);
const commands: Record<string, string[]> = {
  apply: ["install", "--frozen", "--only", "apm"],
  install: ["install", ...args],
  uninstall: ["uninstall", ...args],
  update: ["deps", "update", ...args],
};
const command = commands[operation];
if (!command) throw new Error("unknown APM operation");
const directory = path.join(repository, "home");
await run(["apm", ...command], directory);
await run(["apm", "compile", "--clean"], directory);
await applyCopies(repository, home!, "agents");
