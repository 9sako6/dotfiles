import { describe, expect, test } from "bun:test";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir, writeTree } from "./test-helpers";

const undoScript = path.resolve(import.meta.dir, "../home/mybin/git-undo");
const environment = { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" };

function runGit(root: string, ...args: string[]) {
  return Bun.spawnSync(["git", "-C", root, ...args], { env: environment, timeout: 10_000 });
}

function git(root: string, ...args: string[]) {
  const result = runGit(root, ...args);
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  return result.stdout.toString();
}

function commit(root: string) {
  git(root, "add", ".");
  git(root, "commit", "-qm", "fixture");
}

function undo(root: string, directory = ".", ...args: string[]) {
  return Bun.spawnSync(["/bin/sh", undoScript, ...args], {
    cwd: path.join(root, directory),
    env: environment,
    timeout: 10_000,
  });
}

async function withGitRepo(run: (root: string) => Promise<void>) {
  await withTempDir("git-undo", async (root) => {
    git(root, "init", "-q", "-b", "master");
    git(root, "config", "user.name", "Fixture");
    git(root, "config", "user.email", "fixture@example.invalid");
    git(root, "config", "commit.gpgsign", "false");
    git(root, "config", "core.hooksPath", "/dev/null");
    await writeTree(root, { "sibling/b": "original", "work/a": "original" });
    await run(root);
  });
}

describe("git undo with real Git state", () => {
  test("no arguments unstage the whole repository from a subdirectory", async () => {
    await withGitRepo(async (root) => {
      commit(root);
      const head = git(root, "rev-parse", "HEAD");
      await writeFile(path.join(root, "sibling/b"), "changed");
      git(root, "add", ".");

      const result = undo(root, "work");

      expect(result.exitCode, result.stderr.toString()).toBe(0);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("");
      expect(git(root, "rev-parse", "HEAD")).toBe(head);
      expect(await readFile(path.join(root, "sibling/b"), "utf8")).toBe("changed");
    });
  });

  test("the initial commit can be undone then unstaged without losing files", async () => {
    await withGitRepo(async (root) => {
      commit(root);
      const first = undo(root);
      expect(first.exitCode, first.stderr.toString()).toBe(0);
      expect(runGit(root, "rev-parse", "--verify", "HEAD").exitCode).not.toBe(0);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("sibling/b\nwork/a\n");

      const result = undo(root, "work");

      expect(result.exitCode, result.stderr.toString()).toBe(0);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("");
      expect(runGit(root, "rev-parse", "--verify", "HEAD").exitCode).not.toBe(0);
      for (const name of ["sibling/b", "work/a"]) {
        expect(await readFile(path.join(root, name), "utf8")).toBe("original");
      }
    });
  });

  test("explicit paths remain relative and leave other entries staged", async () => {
    await withGitRepo(async (root) => {
      commit(root);
      const head = git(root, "rev-parse", "HEAD");
      await writeTree(root, { "sibling/b": "changed", "work/a": "changed" });
      git(root, "add", ".");

      const result = undo(root, "work", "a");

      expect(result.exitCode, result.stderr.toString()).toBe(0);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("sibling/b\n");
      expect(git(root, "rev-parse", "HEAD")).toBe(head);
      expect(git(root, "show", ":sibling/b")).toBe("changed");
      for (const name of ["sibling/b", "work/a"]) {
        expect(await readFile(path.join(root, name), "utf8")).toBe("changed");
      }
    });
  });

  test("an unborn explicit path preserves unstaged edits and other index entries", async () => {
    await withGitRepo(async (root) => {
      git(root, "add", ".");
      await writeFile(path.join(root, "work/a"), "edited after staging");

      const result = undo(root, "work", "a");

      expect(result.exitCode, result.stderr.toString()).toBe(0);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("sibling/b\n");
      expect(runGit(root, "rev-parse", "--verify", "HEAD").exitCode).not.toBe(0);
      expect(git(root, "show", ":sibling/b")).toBe("original");
      expect(await readFile(path.join(root, "sibling/b"), "utf8")).toBe("original");
      expect(await readFile(path.join(root, "work/a"), "utf8")).toBe("edited after staging");
    });
  });

  test("a missing staged path does not undo a commit", async () => {
    await withGitRepo(async (root) => {
      commit(root);
      const head = git(root, "rev-parse", "HEAD");

      const result = undo(root, "work", "a");

      expect(result.exitCode).toBe(1);
      expect(git(root, "rev-parse", "HEAD")).toBe(head);
      expect(git(root, "diff", "--cached", "--name-only")).toBe("");
      for (const name of ["sibling/b", "work/a"]) {
        expect(await readFile(path.join(root, name), "utf8")).toBe("original");
      }
    });
  });
});
