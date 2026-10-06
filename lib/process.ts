import { homedir } from "node:os";
import path from "node:path";

export const repository = path.resolve(import.meta.dir, "..");
export const home = homedir();
if (!home || !path.isAbsolute(home)) throw new Error("HOME must be an absolute path");

export async function run(args: string[], cwd = repository, env: NodeJS.ProcessEnv = process.env) {
  const child = Bun.spawn(args, { cwd, env: { ...env, HOME: env.HOME || home }, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  if (await child.exited !== 0) throw new Error(`${args[0]} failed`);
}

export async function capture(args: string[], cwd = repository) {
  const child = Bun.spawn(args, { cwd, env: { ...process.env, HOME: process.env.HOME || home }, stdin: "ignore", stdout: "pipe", stderr: "inherit" });
  const text = await new Response(child.stdout).text();
  if (await child.exited !== 0) throw new Error(`${args[0]} failed`);
  return text.trim();
}
