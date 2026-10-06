import path from "node:path";

export const repository = path.resolve(import.meta.dir, "..");
export const home = process.env.HOME;
if (!home || !path.isAbsolute(home)) throw new Error("HOME must be an absolute path");

export async function run(args: string[], cwd = repository, env: NodeJS.ProcessEnv = process.env) {
  const child = Bun.spawn(args, { cwd, env, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  if (await child.exited !== 0) throw new Error(`${args[0]} failed`);
}

export async function capture(args: string[], cwd = repository) {
  const child = Bun.spawn(args, { cwd, stdin: "ignore", stdout: "pipe", stderr: "inherit" });
  const text = await new Response(child.stdout).text();
  if (await child.exited !== 0) throw new Error(`${args[0]} failed`);
  return text.trim();
}
