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

  test("CI runs a gitleaks secret scan", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");

    expect(workflow).toContain("secret-scan:");
    expect(workflow).toContain("needs: secret-scan");
    expect(workflow).toContain("runs-on: ubuntu-latest");
    expect(workflow).toContain("gitleaks/gitleaks-action@v2");
    expect(workflow).toContain("GITHUB_TOKEN");
    expect(workflow).toContain("fetch-depth: 0");
  });

  test("repo gitignore blocks tracked local shell overrides and secret-like files", async () => {
    const gitignore = await readFile(".gitignore", "utf8");

    expect(gitignore).toContain("dist/.zsh.d/local.zsh");
    expect(gitignore).toContain("dist/.zsh.d/secrets.zsh");
    expect(gitignore).toContain(".env");
    expect(gitignore).toContain("*.pem");
    expect(gitignore).toContain("*.key");
  });

  test("README documents where local zsh overrides and secrets belong", async () => {
    const readme = await readFile("README.md", "utf8");

    expect(readme).toContain("~/.zsh.d/local.zsh");
    expect(readme).toContain("~/.zsh.d/secrets.zsh");
    expect(readme).toContain("dist/");
  });

  test("README documents the managed modern CLI tools and local-only atuin usage", async () => {
    const readme = await readFile("README.md", "utf8");

    expect(readme).toContain("zoxide");
    expect(readme).toContain("atuin");
    expect(readme).toContain("delta");
    expect(readme).toContain("fd");
    expect(readme).toContain("bat");
    expect(readme).toContain("eza");
    expect(readme).toContain("sync");
    expect(readme).toContain("Ctrl-R");
  });
});
