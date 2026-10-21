import { expect, test } from "bun:test";
import { writeFile } from "node:fs/promises";
import path from "node:path";
import { withTempDir } from "./test-helpers";

const filter = path.resolve(import.meta.dir, "../lib/remove-emudhra-roots.awk");

async function removeCompromisedRoots(bundle: string) {
  const child = Bun.spawn(["awk", "-f", filter], {
    stdin: new Blob([bundle]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect(exitCode).toBe(0);
  expect(stderr).toBe("");
  return stdout;
}

test("compromised trust paths are removed without removing adjacent roots", async () => {
  const bundle = [
    "### Comodo CA Limited",
    "=== /O=Comodo CA Limited/CN=AAA Certificate Services",
    "compromised-aaa-root",
    "=== /O=Comodo CA Limited/CN=Unrelated Root",
    "unrelated-root",
    "### eMudhra Technologies Limited",
    "=== /O=eMudhra Technologies Limited/CN=emSign ECC Root CA - G3",
    "compromised-emsign-root",
    "=== /O=eMudhra Technologies Limited/CN=emSign Root CA - G1",
    "unaffected-emsign-root",
    "### Another CA",
    "=== /CN=Another Root",
    "another-root",
    "",
  ].join("\n");
  const filtered = await removeCompromisedRoots(bundle);
  expect(filtered).not.toContain("AAA Certificate Services");
  expect(filtered).not.toContain("emSign ECC Root CA - G3");
  expect(filtered).not.toContain("compromised-");
  expect(filtered).toContain("unrelated-root\n");
  expect(filtered).toContain("emSign Root CA - G1\nunaffected-emsign-root\n");
  expect(filtered).toContain("another-root\n");
  expect(await removeCompromisedRoots(filtered)).toBe(filtered);
});

test("a new organization resets the filter after the last compromised root", async () => {
  const bundle = "=== /CN=emSign ECC Root CA - G3\ncompromised\n### Another CA\nretained\n";
  expect(await removeCompromisedRoots(bundle)).toBe("### Another CA\nretained\n");
});

test("zshenv selects the system trust bundle and preserves explicit client bundles", async () => {
  await withTempDir("trust-bundle", async home => {
    const certificates = path.join(home, "ca.pem");
    await writeFile(certificates, "fixture");
    const config = path.resolve(import.meta.dir, "../home/.zshenv");
    const clients = ["SSL_CERT_FILE", "PIP_CERT", "REQUESTS_CA_BUNDLE", "AWS_CA_BUNDLE"];
    for (const explicit of [false, true]) {
      const overrides = explicit ? Object.fromEntries(clients.map(name => [name, `${certificates}.${name}`])) : {};
      const child = Bun.spawn([
        "/bin/zsh", "-f", "-c",
        'source "$1"; print -rl -- "$SSL_CERT_FILE" "$PIP_CERT" "$REQUESTS_CA_BUNDLE" "$AWS_CA_BUNDLE"',
        "fixture", config,
      ], {
        env: { HOME: home, PATH: "/usr/bin:/bin", NIX_SSL_CERT_FILE: certificates, ...overrides },
        stdout: "pipe",
        stderr: "pipe",
      });
      const [exitCode, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect(exitCode).toBe(0);
      expect(stderr).toBe("");
      expect(stdout.trim().split("\n")).toEqual(clients.map(name => explicit ? overrides[name]! : certificates));
    }
  });
});
