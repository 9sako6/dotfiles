import { describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  managedCheckoutPath,
  parseSystemSourceRequest,
  prepareRemoteCheckout,
  runGit,
  systemSourceDataRoot,
} from "../lib/system-source";

async function withGitSource(
  run: (source: string, dataRoot: string) => Promise<void>,
): Promise<void> {
  const root = await mkdtemp(path.join(os.tmpdir(), "system-source-test-"));
  const source = path.join(root, "source");
  try {
    await runGit(["init", "--initial-branch=master", source]);
    await Bun.write(path.join(source, "flake.nix"), "{ value = 1; }\n");
    await runGit(["-C", source, "add", "flake.nix"]);
    await runGit([
      "-C",
      source,
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.test",
      "commit",
      "-m",
      "initial",
    ]);
    await run(source, path.join(root, "data"));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

describe("system source request", () => {
  test("引数なし、default、remoteを異なる状態として扱う", () => {
    expect(parseSystemSourceRequest(undefined, undefined)).toEqual({ type: "current" });
    expect(parseSystemSourceRequest(undefined, "true")).toEqual({ type: "default" });
    expect(parseSystemSourceRequest("git@example.test:owner/config.git", undefined)).toEqual({
      type: "remote",
      url: "git@example.test:owner/config.git",
    });
  });

  test("defaultとremoteの同時指定を拒否する", () => {
    expect(() => parseSystemSourceRequest("git@example.test:owner/config.git", "true"))
      .toThrow("cannot be used together");
  });

  test("credentialを含まないSSHとHTTPS clone URLを受け付ける", () => {
    for (const url of [
      "git@example.test:owner/config.git",
      "ssh://git@example.test/owner/config.git",
      "https://example.test/owner/config.git",
    ]) {
      expect(parseSystemSourceRequest(url, undefined)).toEqual({ type: "remote", url });
    }
  });

  test("credential、branch指定、local pathを拒否する", () => {
    for (const url of [
      "https://token@example.test/owner/config.git",
      "https://example.test/owner/config.git?ref=main",
      "ssh://git@example.test/owner/config.git#main",
      "../config",
    ]) {
      expect(() => parseSystemSourceRequest(url, undefined)).toThrow();
    }
  });
});

describe("system source paths", () => {
  test("XDG data homeと標準fallbackから管理rootを決める", () => {
    expect(systemSourceDataRoot("/Users/example", "/data")).toBe(
      "/data/dotfiles/nix-darwin",
    );
    expect(systemSourceDataRoot("/Users/example", "relative")).toBe(
      "/Users/example/.local/share/dotfiles/nix-darwin",
    );
  });

  test("remote URLごとに安定した専用checkout pathを作る", () => {
    const root = "/data/dotfiles/nix-darwin";
    const first = managedCheckoutPath(root, "git@example.test:owner/first.git");
    expect(first).toBe(managedCheckoutPath(root, "git@example.test:owner/first.git"));
    expect(first).not.toBe(managedCheckoutPath(root, "git@example.test:owner/second.git"));
    expect(path.dirname(first)).toBe(root);
  });
});

describe("managed system source checkout", () => {
  test("remote既定branchの最新commitへdetached checkoutを揃える", async () => {
    await withGitSource(async (source, dataRoot) => {
      const first = await prepareRemoteCheckout(dataRoot, source);
      expect(await readFile(path.join(first.directory, "flake.nix"), "utf8"))
        .toBe("{ value = 1; }\n");

      await Bun.write(path.join(source, "flake.nix"), "{ value = 2; }\n");
      await runGit(["-C", source, "add", "flake.nix"]);
      await runGit([
        "-C",
        source,
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.test",
        "commit",
        "-m",
        "update",
      ]);

      const second = await prepareRemoteCheckout(dataRoot, source);
      expect(second.revision).not.toBe(first.revision);
      expect(await readFile(path.join(second.directory, "flake.nix"), "utf8"))
        .toBe("{ value = 2; }\n");
    });
  });

  test("管理checkoutのlocal changesを破棄しない", async () => {
    await withGitSource(async (source, dataRoot) => {
      const prepared = await prepareRemoteCheckout(dataRoot, source);
      await Bun.write(path.join(prepared.directory, "local.txt"), "keep\n");
      await expect(prepareRemoteCheckout(dataRoot, source))
        .rejects.toThrow("contains local changes");
    });
  });
});
