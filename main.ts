import { install } from "./src/install.ts";

const homeDir = Deno.env.get("HOME");
if (!homeDir) {
  throw new Error("Fail to get the home directory path.");
}
await install("./dist", homeDir);
