import { createHash } from "node:crypto";
import { access, lstat, mkdir, readlink, rm } from "node:fs/promises";
import path from "node:path";

export type SystemSourceRequest =
  | { type: "current" }
  | { type: "default" }
  | { type: "remote"; url: string };

export function parseSystemCommandArgs(args: string[]): {
  mode: "apply" | "plan";
  request: SystemSourceRequest;
} {
  const [mode, ...sourceArgs] = args;
  if (mode !== "plan" && mode !== "apply") {
    throw new Error("usage: system.ts <plan|apply> [--default|git-url]");
  }

  let gitUrl: string | undefined;
  let useDefault: string | undefined;
  for (const argument of sourceArgs) {
    if (argument === "--default") {
      if (useDefault) throw new Error("--default may only be specified once");
      useDefault = "true";
    } else if (argument.startsWith("-")) {
      throw new Error(`unknown option: ${argument}`);
    } else if (gitUrl) {
      throw new Error("only one Git URL may be specified");
    } else {
      gitUrl = argument;
    }
  }
  return { mode, request: parseSystemSourceRequest(gitUrl, useDefault) };
}

export function parseSystemSourceRequest(
  gitUrl: string | undefined,
  useDefault: string | undefined,
): SystemSourceRequest {
  const defaultRequested = useDefault === "true";
  const url = gitUrl?.trim() ?? "";
  if (defaultRequested && url !== "") {
    throw new Error("a Git URL and --default cannot be used together");
  }
  if (defaultRequested) return { type: "default" };
  if (url === "") return { type: "current" };
  validateGitUrl(url);
  return { type: "remote", url };
}

export function systemSourceDataRoot(homeDir: string, dataHome?: string): string {
  const root = dataHome && path.isAbsolute(dataHome)
    ? dataHome
    : path.join(homeDir, ".local", "share");
  return path.join(root, "dotfiles", "nix-darwin");
}

export function managedCheckoutPath(dataRoot: string, gitUrl: string): string {
  const digest = createHash("sha256").update(gitUrl).digest("hex").slice(0, 24);
  return path.join(dataRoot, digest);
}

export type RunGit = (args: string[]) => Promise<string>;

export type PreparedSystemSource = {
  directory: string;
  kind: "default" | "remote";
  previousTarget: string | null;
  revision: string;
  url: string | null;
};

export async function runGit(args: string[]): Promise<string> {
  const process = Bun.spawn(["git", ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(stderr.trim() || `git exited with status ${exitCode}`);
  }
  return stdout.trim();
}

export async function prepareRemoteCheckout(
  dataRoot: string,
  gitUrl: string,
  git: RunGit = runGit,
): Promise<{ directory: string; revision: string }> {
  const directory = managedCheckoutPath(dataRoot, gitUrl);
  await mkdir(dataRoot, { recursive: true });
  let created = false;

  try {
    try {
      const stat = await lstat(directory);
      if (!stat.isDirectory()) throw new Error("managed checkout is not a directory");
    } catch (error) {
      if (!(error instanceof Error) || !("code" in error) || error.code !== "ENOENT") {
        throw error;
      }
      created = true;
      await git(["clone", "--filter=blob:none", "--no-checkout", "--", gitUrl, directory]);
    }

    await access(path.join(directory, ".git"));
    if (await git(["-C", directory, "remote", "get-url", "origin"]) !== gitUrl) {
      throw new Error("managed checkout origin does not match its source URL");
    }
    if (!created && await git(["-C", directory, "status", "--porcelain"]) !== "") {
      throw new Error("managed checkout contains local changes");
    }
    await git(["-C", directory, "fetch", "--quiet", "origin"]);
    await git(["-C", directory, "remote", "set-head", "origin", "--auto"]);
    const revision = await git(["-C", directory, "rev-parse", "refs/remotes/origin/HEAD"]);
    await git(["-C", directory, "checkout", "--quiet", "--detach", revision]);
    await access(path.join(directory, "flake.nix"));
    return { directory, revision };
  } catch (error) {
    if (created) await rm(directory, { recursive: true, force: true });
    throw error;
  }
}

export async function resolveSystemSource(
  request: SystemSourceRequest,
  paths: { dataRoot: string; publicDirectory: string; selectionPath: string },
  git: RunGit = runGit,
): Promise<PreparedSystemSource> {
  const selected = await inspectSelectedSystemSource(paths, git);
  const desired = request.type === "current"
    ? selected.kind === "default" ? { type: "default" as const } : {
      type: "remote" as const,
      url: selected.url!,
    }
    : request;

  if (desired.type === "default") {
    await access(path.join(paths.publicDirectory, "flake.nix"));
    return {
      directory: paths.publicDirectory,
      kind: "default",
      previousTarget: selected.previousTarget,
      revision: await git(["-C", paths.publicDirectory, "rev-parse", "HEAD"]),
      url: null,
    };
  }

  const prepared = await prepareRemoteCheckout(paths.dataRoot, desired.url, git);
  return {
    ...prepared,
    kind: "remote",
    previousTarget: selected.previousTarget,
    url: desired.url,
  };
}

export async function inspectSelectedSystemSource(
  paths: { dataRoot: string; publicDirectory: string; selectionPath: string },
  git: RunGit = runGit,
): Promise<PreparedSystemSource> {
  const publicFlake = path.join(paths.publicDirectory, "flake.nix");
  const selected = await inspectSelection(publicFlake, paths.dataRoot, paths.selectionPath, git);
  if (selected.request.type === "default") {
    await access(publicFlake);
    return {
      directory: paths.publicDirectory,
      kind: "default",
      previousTarget: selected.target,
      revision: await git(["-C", paths.publicDirectory, "rev-parse", "HEAD"]),
      url: null,
    };
  }
  const directory = managedCheckoutPath(paths.dataRoot, selected.request.url);
  await access(path.join(directory, "flake.nix"));
  return {
    directory,
    kind: "remote",
    previousTarget: selected.target,
    revision: await git(["-C", directory, "rev-parse", "HEAD"]),
    url: selected.request.url,
  };
}

async function inspectSelection(
  publicFlake: string,
  dataRoot: string,
  selectionPath: string,
  git: RunGit,
): Promise<{
  request: { type: "default" } | { type: "remote"; url: string };
  target: string | null;
}> {
  let target: string;
  try {
    const stat = await lstat(selectionPath);
    if (!stat.isSymbolicLink()) throw new Error("system source selection is not a symlink");
    const value = await readlink(selectionPath);
    target = path.resolve(path.dirname(selectionPath), value);
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT") {
      return { request: { type: "default" }, target: null };
    }
    throw error;
  }

  if (target === publicFlake) return { request: { type: "default" }, target };
  const checkout = path.dirname(target);
  if (
    path.basename(target) !== "flake.nix" ||
    path.dirname(checkout) !== dataRoot ||
    !/^[0-9a-f]{24}$/.test(path.basename(checkout))
  ) {
    throw new Error("system source selection is not managed by dotfiles");
  }
  const url = await git(["-C", checkout, "remote", "get-url", "origin"]);
  validateGitUrl(url);
  if (managedCheckoutPath(dataRoot, url) !== checkout) {
    throw new Error("system source selection does not match its origin");
  }
  return { request: { type: "remote", url }, target };
}

function validateGitUrl(value: string): void {
  if (/[\0\r\n\s]/.test(value)) {
    throw new Error("Git URL must not contain whitespace or control characters");
  }
  if (/^[^/@:]+@[^/:]+:.+/.test(value)) return;

  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Git source must be an SSH or HTTPS clone URL");
  }
  if (url.protocol !== "ssh:" && url.protocol !== "https:") {
    throw new Error("Git source must use SSH or HTTPS");
  }
  if (url.hostname === "" || url.pathname === "" || url.pathname === "/") {
    throw new Error("Git source must identify a remote repository");
  }
  if (url.password !== "" || (url.protocol === "https:" && url.username !== "")) {
    throw new Error("Git credentials must not be embedded in the URL");
  }
  if (url.search !== "" || url.hash !== "") {
    throw new Error("Git source must use the remote default branch");
  }
}
