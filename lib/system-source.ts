import { createHash } from "node:crypto";
import path from "node:path";

export type SystemSourceRequest =
  | { type: "current" }
  | { type: "default" }
  | { type: "remote"; url: string };

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
