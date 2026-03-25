#!/usr/bin/env bun

import path from "node:path";
import { runSetup } from "./lib/setup";

async function main() {
  const homeDir = process.env.HOME;
  if (!homeDir) {
    throw new Error("HOME is not set");
  }

  await runSetup({
    homeDir,
    repoRoot: path.resolve(process.cwd()),
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
