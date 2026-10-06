import { expect, test } from "bun:test";
import { stat } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers.ts";

const repository = path.resolve(import.meta.dir, "..");
const probe = `
  const { home, capture } = await import("./lib/process.ts");
  const inherited = await capture([process.execPath, "--no-env-file", "-e", "console.log(process.env.HOME)"]);
  console.log(JSON.stringify({ home, inherited }));
`;

async function execute(env: NodeJS.ProcessEnv) {
  const child = Bun.spawn([process.execPath, "--no-env-file", "-e", probe], {
    cwd: repository,
    env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { code, stdout, stderr };
}

for (const value of [undefined, ""]) {
  test(`${value === undefined ? "missing" : "empty"} HOME resolves the account home and passes it to child commands`, async () => {
    const env = { ...process.env };
    if (value === undefined) delete env.HOME;
    else env.HOME = value;
    const result = await execute(env);
    expect(result.code).toBe(0);
    const output = JSON.parse(result.stdout);
    expect(path.isAbsolute(output.home)).toBe(true);
    expect((await stat(output.home)).isDirectory()).toBe(true);
    expect(output.inherited).toBe(output.home);
  });
}

test("explicit HOME remains the deployment and child command home", async () => {
  await withTempDir("process-home", async taskHome => {
    const result = await execute({ ...process.env, HOME: taskHome });
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual({ home: taskHome, inherited: taskHome });
  });
});

test("relative HOME stops before child commands run", async () => {
  const result = await execute({ ...process.env, HOME: "relative" });
  expect(result.code).not.toBe(0);
  expect(result.stdout).toBe("");
  expect(result.stderr).toContain("HOME must be an absolute path");
});
