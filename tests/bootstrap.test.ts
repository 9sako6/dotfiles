import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

describe("bootstrap trust flow", () => {
  test("install.sh trusts the repo before running mise tasks", async () => {
    const installScript = await readFile("install.sh", "utf8");

    expect(installScript).toContain("\"$MISE_BIN\" trust");
    expect(installScript).not.toContain("\"$MISE_BIN\" use -g");
    expect(installScript).toContain("\"$MISE_BIN\" install");
    expect(installScript).toContain("\"$MISE_BIN\" run setup");
  });

  test("install.sh installs global mise tools after dist is linked", async () => {
    const installScript = await readFile("install.sh", "utf8");
    const installCount = installScript.split("\"$MISE_BIN\" install").length - 1;

    expect(installCount).toBe(2);
    expect(installScript.indexOf("\"$MISE_BIN\" run setup")).toBeLessThan(
      installScript.lastIndexOf("\"$MISE_BIN\" install"),
    );
  });

  test("CI trusts the repo before installing bun", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");

    expect(workflow).toContain("mise trust");
    expect(workflow).toContain("bun test");
    expect(workflow).toContain("bun run scripts/link-dist.ts --check");
  });

  test("CI installs only bun for verification", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");

    expect(workflow).toContain("mise install bun");
    expect(workflow).not.toContain("run: mise install\n");
    expect(workflow).not.toContain("mise run test");
    expect(workflow).not.toContain("mise run link:check");
  });
});
