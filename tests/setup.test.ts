import { describe, expect, test } from "bun:test";
import path from "node:path";
import { parseCliArgs } from "../scripts/lib/paths";
import { buildSetupPlan } from "../scripts/lib/setup";

describe("parseCliArgs", () => {
  test("treats --check as dry-run", () => {
    expect(parseCliArgs(["--check"])).toEqual({ dryRun: true });
  });

  test("treats --dry-run as dry-run", () => {
    expect(parseCliArgs(["--dry-run"])).toEqual({ dryRun: true });
  });

  test("defaults to apply mode", () => {
    expect(parseCliArgs([])).toEqual({ dryRun: false });
  });
});

describe("buildSetupPlan", () => {
  test("always includes the dist linking step", () => {
    expect(
      buildSetupPlan({
        homeDir: "/tmp/home",
        repoRoot: "/tmp/repo",
        skipExtraSetup: false,
        zinitInstalled: true,
      }),
    ).toEqual([
      {
        kind: "link-dist",
        sourceRoot: path.join("/tmp/repo", "dist"),
      },
    ]);
  });

  test("adds zinit installation when it is missing", () => {
    expect(
      buildSetupPlan({
        homeDir: "/tmp/home",
        repoRoot: "/tmp/repo",
        skipExtraSetup: false,
        zinitInstalled: false,
      }),
    ).toEqual([
      {
        kind: "link-dist",
        sourceRoot: path.join("/tmp/repo", "dist"),
      },
      {
        homeDir: "/tmp/home",
        kind: "install-zinit",
      },
    ]);
  });

  test("can skip extra setup explicitly", () => {
    expect(
      buildSetupPlan({
        homeDir: "/tmp/home",
        repoRoot: "/tmp/repo",
        skipExtraSetup: true,
        zinitInstalled: false,
      }),
    ).toEqual([
      {
        kind: "link-dist",
        sourceRoot: path.join("/tmp/repo", "dist"),
      },
    ]);
  });
});
