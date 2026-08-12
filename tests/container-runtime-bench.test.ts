import { describe, expect, test } from "bun:test";
import { median, parseMetricCsv, renderReport, resolvePathInHome } from "../bench/container-runtime/bench";

describe("container runtime benchmark report", () => {
  test("parses metric rows", () => {
    expect(parseMetricCsv("colima,seq_write_seconds,1.25,seconds,2\n")).toEqual([
      { target: "colima", metric: "seq_write_seconds", value: 1.25, unit: "seconds", iteration: 2 },
    ]);
  });

  test("calculates median for odd and even sample counts", () => {
    expect(median([9, 1, 3])).toBe(3);
    expect(median([1, 3, 5, 7])).toBe(4);
  });

  test("renders grouped statistics", () => {
    const report = renderReport([
      { target: "apple-home", metric: "git_status_seconds", value: 2, unit: "seconds", iteration: 1 },
      { target: "apple-home", metric: "git_status_seconds", value: 4, unit: "seconds", iteration: 2 },
    ], { cpus: 4 });
    expect(report).toContain("| apple-home | git_status_seconds | 3.000 | 2.000 | 4.000 | seconds | 2 |");
  });

  test("only maps paths inside the shared home", () => {
    expect(resolvePathInHome("/Users/me", "/Users/me/ghq/repo")).toBe("ghq/repo");
    expect(() => resolvePathInHome("/Users/me", "/tmp/repo")).toThrow();
  });
});
