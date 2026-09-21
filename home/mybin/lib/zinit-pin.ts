import { readFile } from "node:fs/promises";
import path from "node:path";

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

async function main(): Promise<void> {
  const [configPath, destination, ...extra] = process.argv.slice(2);
  const home = process.env.HOME;
  if (!configPath || !destination || extra.length || !home) {
    throw new Error("expected a mise configuration, plugin directory and HOME");
  }
  const config = Bun.TOML.parse(await readFile(configPath, "utf8"));
  if (!record(config)) throw new Error("mise configuration must be a table");
  const bootstrap = config.bootstrap;
  const repos = record(bootstrap) ? bootstrap.repos : undefined;
  if (!record(repos)) throw new Error("mise configuration has no bootstrap.repos");
  const matches = Object.entries(repos).filter(([directory]) => {
    const expanded = directory.startsWith("~/") ? path.join(home, directory.slice(2)) : directory;
    return path.isAbsolute(expanded) && path.resolve(expanded) === path.resolve(destination);
  });
  if (matches.length !== 1) throw new Error("plugin directory must have exactly one pin");
  const specification = matches[0][1];
  const revision = record(specification) ? specification.ref : undefined;
  if (typeof revision !== "string" || !/^[0-9a-f]{40}$/.test(revision)) {
    throw new Error("plugin pin must be a complete Git commit hash");
  }
  process.stdout.write(`${revision}\n`);
}

main().catch((error: unknown) => {
  console.error(`zinit: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
});
