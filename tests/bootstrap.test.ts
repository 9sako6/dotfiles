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
    expect(installScript).not.toContain("brew install --cask");
    expect(installScript).not.toContain("brew bundle");
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

  test("README stays minimal while linking to durable Japanese docs", async () => {
    const readme = await readFile("README.md", "utf8");

    expect(readme).toContain("curl -fsSL dot.9sako6.com | bash");
    expect(readme).toContain("See:");
    expect(readme).toContain("docs/repo-map.md");
    expect(readme).toContain("docs/operations.md");
    expect(readme).not.toContain("mise run link:check");
    expect(readme).not.toContain("Brewfile");
    expect(readme).not.toContain(".dotfiles-backups");
  });

  test("repo map documents the managed surface in Japanese", async () => {
    const repoMap = await readFile("docs/repo-map.md", "utf8");

    expect(repoMap).toContain("このリポジトリ");
    expect(repoMap).toContain("dist/");
    expect(repoMap).toContain("scripts/");
    expect(repoMap).toContain("tests/");
    expect(repoMap).toContain("秘密情報");
  });

  test("operations guide documents safe commands and boundaries in Japanese", async () => {
    const operations = await readFile("docs/operations.md", "utf8");

    expect(operations).toContain("安全");
    expect(operations).toContain("mise run doctor");
    expect(operations).toContain("mise run link:check");
    expect(operations).toContain("install.sh");
    expect(operations).toContain("dist/");
  });
});
