export const dotCommands = ["apply", "doctor", "plan", "pull"] as const;

export type DotCommand = typeof dotCommands[number];

export type ParsedDotCommand =
  | { command?: DotCommand; type: "help" }
  | { command: DotCommand; type: "run" };

export class DotUsageError extends Error {}

const helpArguments = new Set(["-h", "--help", "help"]);

export function parseDotCommand(args: string[]): ParsedDotCommand {
  if (args.length === 0 || (args.length === 1 && helpArguments.has(args[0]!))) {
    return { type: "help" };
  }

  const [first, second] = args;
  if (first === "help") {
    if (args.length !== 2) {
      throw new DotUsageError("help expects exactly one command");
    }
    return { command: requireDotCommand(second!), type: "help" };
  }

  const command = requireDotCommand(first!);
  if (args.length === 1) {
    return { command, type: "run" };
  }
  if (args.length === 2 && helpArguments.has(second!)) {
    return { command, type: "help" };
  }
  throw new DotUsageError(`unexpected arguments for '${command}'`);
}

export function formatDotHelp(command?: DotCommand): string {
  if (command === undefined) {
    return [
      "Usage: dot <command>",
      "",
      "Commands:",
      "  pull    Fast-forward the local dotfiles checkout from origin/master",
      "  plan    Show planned home-directory deployment changes",
      "  apply   Review and apply home-directory deployment changes",
      "  doctor  Diagnose deployment and package-manager drift",
      "",
      "Run 'dot help <command>' for details.",
    ].join("\n");
  }

  const descriptions: Record<DotCommand, string> = {
    apply: "Review and apply home-directory deployment changes.",
    doctor: "Diagnose deployment, mise, and Homebrew drift.",
    plan: "Show planned home-directory deployment changes without modifying files.",
    pull: "Fast-forward the clean local master branch from origin/master.",
  };
  return `Usage: dot ${command}\n\n${descriptions[command]}`;
}

export async function hasUncommittedChanges(repoRoot: string): Promise<boolean> {
  return (await git(repoRoot, ["status", "--porcelain", "--untracked-files=all"])) !== "";
}

export async function pullDotfiles(repoRoot: string): Promise<string> {
  const branch = await git(repoRoot, ["branch", "--show-current"]);
  if (branch !== "master") {
    throw new Error(
      `pull requires branch 'master'; current checkout is ${branch === "" ? "detached" : `'${branch}'`}`,
    );
  }

  const upstream = await git(repoRoot, ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]);
  if (upstream !== "origin/master") {
    throw new Error(`pull requires upstream 'origin/master'; current upstream is '${upstream}'`);
  }
  if (await hasUncommittedChanges(repoRoot)) {
    throw new Error("pull requires a clean worktree");
  }

  await git(repoRoot, ["fetch", "--quiet", "origin", "master"]);
  const localRevision = await git(repoRoot, ["rev-parse", "HEAD"]);
  const remoteRevision = await git(repoRoot, ["rev-parse", "origin/master"]);

  if (localRevision === remoteRevision) {
    return "Dotfiles are up to date.";
  }
  if (await isAncestor(repoRoot, localRevision, remoteRevision)) {
    const count = await git(repoRoot, ["rev-list", "--count", `${localRevision}..${remoteRevision}`]);
    await git(repoRoot, ["merge", "--ff-only", "--quiet", "origin/master"]);
    return [
      `Updated dotfiles: ${shortRevision(localRevision)} → ${shortRevision(remoteRevision)} (${formatCommitCount(count)}).`,
      "Run 'dot plan' to review deployment changes.",
    ].join("\n");
  }
  if (await isAncestor(repoRoot, remoteRevision, localRevision)) {
    const count = await git(repoRoot, ["rev-list", "--count", `${remoteRevision}..${localRevision}`]);
    return `Dotfiles are ${formatCommitCount(count)} ahead of origin/master; nothing to pull.`;
  }
  throw new Error("local master and origin/master have diverged");
}

function requireDotCommand(value: string): DotCommand {
  if ((dotCommands as readonly string[]).includes(value)) {
    return value as DotCommand;
  }
  throw new DotUsageError(`unknown command '${value}'`);
}

async function isAncestor(repoRoot: string, ancestor: string, descendant: string): Promise<boolean> {
  const result = await runGit(repoRoot, ["merge-base", "--is-ancestor", ancestor, descendant]);
  if (result.exitCode === 0) return true;
  if (result.exitCode === 1) return false;
  throw gitError(result.stderr, ["merge-base", "--is-ancestor", ancestor, descendant]);
}

async function git(repoRoot: string, args: string[]): Promise<string> {
  const result = await runGit(repoRoot, args);
  if (result.exitCode !== 0) {
    throw gitError(result.stderr, args);
  }
  return result.stdout.trim();
}

async function runGit(repoRoot: string, args: string[]) {
  const child = Bun.spawn(["git", "-C", repoRoot, ...args], {
    stderr: "pipe",
    stdout: "pipe",
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { exitCode, stderr, stdout };
}

function gitError(stderr: string, args: string[]): Error {
  const detail = stderr.trim();
  return new Error(detail === "" ? `git ${args.join(" ")} failed` : detail);
}

function shortRevision(revision: string): string {
  return revision.slice(0, 7);
}

function formatCommitCount(count: string): string {
  return `${count} ${count === "1" ? "commit" : "commits"}`;
}
