import { describe, expect, test } from "bun:test";
import path from "node:path";
import {
  managedCheckoutPath,
  parseSystemSourceRequest,
  systemSourceDataRoot,
} from "../lib/system-source";

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
