import { describe, expect, test } from "bun:test";
import {
  inspectHomebrew,
  inspectMise,
} from "../scripts/lib/doctor";

describe("doctorのmise診断", () => {
  test("miseがprunableと判定したインストールを列挙する", () => {
    const result = inspectMise(JSON.stringify({
      bun: [
        { active: false, installed: true, version: "1.3.11" },
      ],
      terraform: [
        { active: false, installed: true, version: "1.14.3" },
      ],
    }));

    expect(result.prunable).toEqual(["bun@1.3.11", "terraform@1.14.3"]);
  });
});

describe("doctorのHomebrew診断", () => {
  test("宣言済み、未導入、宣言外を区別する", () => {
    const result = inspectHomebrew({
      declaredCasks: new Set(["bitwarden", "ghostty"]),
      declaredFormulae: new Set(["git"]),
      installedCasks: ["bitwarden", "codex"],
      installedFormulae: ["gh", "git"],
    });

    expect(result.missing).toEqual(["brew-cask:ghostty"]);
    expect(result.unmanaged).toEqual(["brew-cask:codex", "brew:gh"]);
  });
});
