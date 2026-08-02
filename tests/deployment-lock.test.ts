import { describe, expect, test } from "bun:test";
import { access, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { withDeploymentLock } from "../scripts/lib/deployment-state";
import { withTempDir } from "./test-helpers";

describe("配備の排他制御", () => {
  test("実行中の配備と同じ台帳を使う処理を拒否し、完了後にlockを解放する", async () => {
    await withTempDir("deployment-lock", async (tempDir) => {
      const statePath = path.join(tempDir, "state", "deployment.json");
      const lockPath = `${statePath}.lock`;
      let markEntered: (() => void) | undefined;
      const entered = new Promise<void>((resolve) => {
        markEntered = resolve;
      });
      let release: (() => void) | undefined;
      const blocked = new Promise<void>((resolve) => {
        release = resolve;
      });
      const first = withDeploymentLock(statePath, async () => {
        markEntered?.();
        await blocked;
      });

      await entered;
      await expect(access(lockPath)).resolves.toBeNull();
      await expect(withDeploymentLock(statePath, async () => undefined)).rejects.toThrow(
        /already running/,
      );

      release?.();
      await first;
      await expect(access(lockPath)).rejects.toThrow();
      await expect(withDeploymentLock(statePath, async () => "completed")).resolves.toBe(
        "completed",
      );
    });
  });

  test("異常終了した所有者のlockを引き継ぐ", async () => {
    await withTempDir("deployment-lock", async (tempDir) => {
      const statePath = path.join(tempDir, "state", "deployment.json");
      const lockPath = `${statePath}.lock`;
      await mkdir(path.dirname(lockPath), { recursive: true });
      await writeFile(lockPath, '{"pid":2147483647}\n');

      await expect(withDeploymentLock(statePath, async () => "recovered")).resolves.toBe(
        "recovered",
      );
      await expect(access(lockPath)).rejects.toThrow();
    });
  });
});
