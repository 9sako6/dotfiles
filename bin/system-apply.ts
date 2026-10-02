import { mkdir, realpath } from "node:fs/promises";
import path from "node:path";
import { safeParents } from "../lib/home-copy.ts";
import { capture, home, repository, run } from "../lib/process.ts";
import { deployResources, nightShift } from "../lib/system-resources.ts";

if (process.platform !== "darwin" || process.arch !== "arm64") throw new Error("system:apply requires Apple Silicon macOS");
if (await capture(["/usr/bin/id", "-u"]) === "0") throw new Error("run system:apply as the login user");
const settings = await nightShift(repository);
const nix = await capture(["/bin/sh", "-c", '. "$1/lib/install-system.sh"; install_system_ensure_lix "$1/bin/install-lix.sh"', "system:apply", repository]);
const anchor = process.env.XDG_STATE_HOME ?? home!;
const state = path.join(process.env.XDG_STATE_HOME ?? path.join(home!, ".local/state"), "dotfiles");
await safeParents(anchor, path.join(state, "current"));
await mkdir(state, { recursive: true });
const user = await capture(["/usr/bin/id", "-un"]);
await run([nix, "--extra-experimental-features", "nix-command flakes", "build", "--impure", "--no-update-lock-file", "--no-write-lock-file", "--file", "nix/apply.nix", "--argstr", "directory", repository, "--argstr", "user", user, "bundle", "--out-link", path.join(state, "current")]);
const system = await realpath(path.join(state, "current/system"));
const resources = await realpath(path.join(state, "current/resources"));
await deployResources(home!, state, resources);
if (settings) {
  const helper = path.join(resources, "bin/nightlight");
  await run([helper, "schedule", settings.start, settings.end]);
  await run([helper, "temp", String(settings.temperature)]);
}
await run(["/usr/bin/sudo", `${path.dirname(nix)}/nix-env`, "--profile", "/nix/var/nix/profiles/system", "--set", system]);
await run(["/usr/bin/sudo", `${system}/sw/bin/darwin-rebuild`, "activate"]);
