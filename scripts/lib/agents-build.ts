import { access, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";

export type AgentsBuildOperation = "build" | "install" | "update" | "uninstall";

export type CommandPlan = {
  command: string;
  args: string[];
};

const compileCommand: CommandPlan = {
  command: "apm",
  args: ["compile", "--clean", "--target", "claude,codex"],
};

export function createAgentsBuildPlan(operation: AgentsBuildOperation, args: string[]): CommandPlan[] {
  switch (operation) {
    case "build":
      return [
        { command: "apm", args: ["install", "--frozen", "--only", "apm", "--target", "claude,codex"] },
        compileCommand,
      ];
    case "install":
      return [{ command: "apm", args: ["install", ...args] }, compileCommand];
    case "update":
      return [{ command: "apm", args: ["deps", "update", ...args] }, compileCommand];
    case "uninstall":
      return [{ command: "apm", args: ["uninstall", ...args] }, compileCommand];
  }
}

export async function finalizeCompiledAgents(rootDir: string) {
  const sourceAgentsPath = path.join(rootDir, "AGENTS.md");
  const codexDir = path.join(rootDir, ".codex");
  const codexAgentsPath = path.join(codexDir, "AGENTS.md");

  await assertExists(sourceAgentsPath, "APM did not generate AGENTS.md");
  await mkdir(codexDir, { recursive: true });
  await rename(sourceAgentsPath, codexAgentsPath);

  await Promise.all([
    rm(path.join(rootDir, "CLAUDE.md"), { force: true }),
    rm(path.join(rootDir, "GEMINI.md"), { force: true }),
    rm(path.join(rootDir, ".codex", "config.toml"), { force: true }),
    rm(path.join(rootDir, ".mcp.json"), { force: true }),
  ]);

  await assertExists(codexAgentsPath, "APM finalization did not produce .codex/AGENTS.md");
}

export async function readFileIfExists(filePath: string) {
  try {
    return await readFile(filePath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

export async function restoreLockfileIfOnlyGeneratedAtChanged(
  lockPath: string,
  originalContent: string | null,
) {
  if (originalContent === null) {
    return;
  }

  const currentContent = await readFileIfExists(lockPath);
  if (
    currentContent !== null &&
    currentContent !== originalContent &&
    ignoreGeneratedAt(currentContent) === ignoreGeneratedAt(originalContent)
  ) {
    await writeFile(lockPath, originalContent);
  }
}

function ignoreGeneratedAt(content: string) {
  return content.replace(/^generated_at: .*$/m, "generated_at: <ignored>");
}

async function assertExists(filePath: string, message: string) {
  try {
    await access(filePath);
  } catch {
    throw new Error(message);
  }
}
