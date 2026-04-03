import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

function expectInOrder(text: string, fragments: string[]) {
  let cursor = -1;

  for (const fragment of fragments) {
    const next = text.indexOf(fragment, cursor + 1);
    expect(next).toBeGreaterThan(cursor);
    cursor = next;
  }
}

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

    expectInOrder(installScript, [
      "\"$MISE_BIN\" trust",
      "\"$MISE_BIN\" run setup",
      "\"$MISE_BIN\" install",
    ]);
    expect(installScript).not.toContain("brew install --cask");
    expect(installScript).not.toContain("brew bundle");
  });

  test("CI trusts the repo before installing dependencies and tests", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");

    expect(workflow).toContain("mise trust");
    expect(workflow).toContain("bun test");
    expect(workflow).toContain("bun run scripts/link-dist.ts --check");
    expect(workflow).toContain("pnpm install --frozen-lockfile");
    expectInOrder(workflow, [
      "run: mise trust",
      "run: |\n          mise install",
      "run: pnpm install --frozen-lockfile",
      "run: bun test",
    ]);
  });

  test("CI installs the repo-local toolchain instead of relying on mise tasks", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");

    expect(workflow).toMatch(/mise install[^\n]*\bnode\b/);
    expect(workflow).toMatch(/mise install[^\n]*\bpnpm\b/);
    expect(workflow).toMatch(/mise install[^\n]*\bbun\b/);
    expect(workflow).not.toContain("mise run test");
    expect(workflow).not.toContain("mise run link:check");
  });

  test("CI pins actions by commit SHA and uses least privilege permissions", async () => {
    const workflow = await readFile(".github/workflows/test.yml", "utf8");
    const checkoutPins = workflow.match(/actions\/checkout@[0-9a-f]{40}/g) ?? [];

    expect(workflow).toContain("permissions:");
    expect(workflow).toContain("contents: read");
    expect(workflow).toContain("secret-scan:");
    expect(workflow).toContain("needs: secret-scan");
    expect(workflow).toContain("runs-on: ubuntu-latest");
    expect(checkoutPins).toHaveLength(2);
    expect(workflow).toMatch(/gitleaks\/gitleaks-action@[0-9a-f]{40}/);
    expect(workflow).not.toContain("actions/checkout@v");
    expect(workflow).not.toContain("gitleaks/gitleaks-action@v");
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
