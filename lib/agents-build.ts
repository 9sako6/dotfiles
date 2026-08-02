import { access, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";

export type AgentsBuildOperation = "build" | "install" | "update" | "uninstall";
export type AgentsOperation = AgentsBuildOperation | "remove-local";

export type CommandPlan = {
  command: string;
  args: string[];
};

export type CommandRunner = (command: string, args: string[], cwd: string) => Promise<void>;

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
      return [{ command: "apm", args: ["install", ...args, "--target", "claude,codex"] }, compileCommand];
    case "update":
      return [{ command: "apm", args: ["deps", "update", ...args] }, compileCommand];
    case "uninstall": {
      const localSkillName = args.map(parseLocalSkillDependency).find((name) => name !== null);
      if (localSkillName !== undefined) {
        throw new Error(
          `Local skill sources are repository-owned; use mise run agents:remove-local ${localSkillName}`,
        );
      }
      return [{ command: "apm", args: ["uninstall", ...args] }, compileCommand];
    }
  }
}

export async function removeLocalSkill(cwd: string, args: string[], runCommand: CommandRunner) {
  const skillName = parseLocalSkillName(args);
  const sourcePath = path.join(cwd, ".apm", "skills", skillName);
  await assertExists(
    path.join(sourcePath, "SKILL.md"),
    `Local skill not found: .apm/skills/${skillName}`,
  );

  const dependency = `./.apm/skills/${skillName}`;
  if (!(await hasApmDependency(path.join(cwd, "apm.yml"), dependency))) {
    await runCommand(
      "apm",
      ["install", dependency, "--target", "claude,codex"],
      cwd,
    );
  }
  await runCommand("apm", ["uninstall", dependency], cwd);

  for (const command of createAgentsBuildPlan("build", [])) {
    await runCommand(command.command, command.args, cwd);
  }
  await rm(sourcePath, { recursive: true });
}

export async function assertRemoteUninstallTargets(cwd: string, args: string[]) {
  for (const value of args) {
    const skillName =
      parseLocalSkillDependency(value) ??
      (/^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/.test(value) ? value : null);
    if (
      skillName !== null &&
      (await exists(path.join(cwd, ".apm", "skills", skillName, "SKILL.md")))
    ) {
      throw new Error(
        `Local skill sources are repository-owned; use mise run agents:remove-local ${skillName}`,
      );
    }
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

function parseLocalSkillName(args: string[]) {
  const [skillName] = args;
  if (
    args.length !== 1 ||
    skillName === undefined ||
    !/^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/.test(skillName)
  ) {
    throw new Error("Expected exactly one local skill name");
  }
  return skillName;
}

function parseLocalSkillDependency(value: string) {
  const match = /^(?:\.\/)?\.apm\/skills\/([^/]+)$/.exec(value);
  return match?.[1] ?? null;
}

async function hasApmDependency(configPath: string, dependency: string) {
  const config = Bun.YAML.parse(await readFile(configPath, "utf8")) as {
    dependencies?: { apm?: unknown };
  };
  const dependencies = config.dependencies?.apm;
  return Array.isArray(dependencies) && dependencies.includes(dependency);
}

async function assertExists(filePath: string, message: string) {
  try {
    await access(filePath);
  } catch {
    throw new Error(message);
  }
}

async function exists(filePath: string) {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}
