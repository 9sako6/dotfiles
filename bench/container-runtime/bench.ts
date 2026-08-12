import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

export type MetricRow = {
  target: string;
  metric: string;
  value: number;
  unit: string;
  iteration: number;
};

type Options = {
  command: "prepare" | "run";
  cpus: number;
  memoryGiB: number;
  iterations: number;
  ioMiB: number;
  fileCount: number;
  repo?: string;
  recreate: boolean;
  colimaProfile: string;
  appleMachine: string;
  image: string;
  output?: string;
};

type CommandResult = {
  stdout: string;
  stderr: string;
  exitCode: number;
};

const scriptDir = path.dirname(import.meta.path);
const workloadPath = path.join(scriptDir, "workload.sh");
const hostHome = homedir();

function usage(): never {
  console.error(`Usage:
  bun bench/container-runtime/bench.ts prepare [options]
  bun bench/container-runtime/bench.ts run [options]

Options:
  --cpus <n>             VM CPU count (default: 4)
  --memory <GiB>         VM memory in GiB (default: 8)
  --iterations <n>       Repetitions per metric (default: 5)
  --io-mib <MiB>         Sequential/fio test size (default: 256)
  --file-count <n>       Small files for metadata tests (default: 10000)
  --repo <path>          Add git status measurements for this repository
  --output <path>        Result directory (run only)
  --recreate             Recreate the dedicated Apple machine (prepare only)
  --colima-profile <id>  Dedicated Colima profile (default: runtime-bench-vz)
  --apple-machine <id>   Dedicated Apple machine (default: runtime-bench)
  --image <ref>          Benchmark image tag (default: runtime-bench:rust-1.88.0)
`);
  process.exit(2);
}

function positiveInt(value: string, flag: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${flag} must be a positive integer`);
  return parsed;
}

function parseArgs(argv: string[]): Options {
  const command = argv.shift();
  if (command !== "prepare" && command !== "run") usage();

  const options: Options = {
    command,
    cpus: 4,
    memoryGiB: 8,
    iterations: 5,
    ioMiB: 256,
    fileCount: 10000,
    recreate: false,
    colimaProfile: "runtime-bench-vz",
    appleMachine: "runtime-bench",
    image: "runtime-bench:rust-1.88.0",
  };

  while (argv.length > 0) {
    const flag = argv.shift()!;
    const take = () => argv.shift() ?? usage();
    switch (flag) {
      case "--cpus": options.cpus = positiveInt(take(), flag); break;
      case "--memory": options.memoryGiB = positiveInt(take(), flag); break;
      case "--iterations": options.iterations = positiveInt(take(), flag); break;
      case "--io-mib": options.ioMiB = positiveInt(take(), flag); break;
      case "--file-count": options.fileCount = positiveInt(take(), flag); break;
      case "--repo": options.repo = path.resolve(take()); break;
      case "--output": options.output = path.resolve(take()); break;
      case "--recreate": options.recreate = true; break;
      case "--colima-profile": options.colimaProfile = take(); break;
      case "--apple-machine": options.appleMachine = take(); break;
      case "--image": options.image = take(); break;
      case "-h":
      case "--help": usage();
      default: throw new Error(`unknown option: ${flag}`);
    }
  }
  return options;
}

function spawn(command: string[], env: Record<string, string> = {}, live = false): CommandResult {
  const result = Bun.spawnSync({
    cmd: command,
    cwd: scriptDir,
    env: { ...process.env, ...env },
    stdout: live ? "inherit" : "pipe",
    stderr: live ? "inherit" : "pipe",
  });
  return {
    stdout: live ? "" : result.stdout.toString(),
    stderr: live ? "" : result.stderr.toString(),
    exitCode: result.exitCode,
  };
}

function checked(command: string[], env: Record<string, string> = {}, live = false): string {
  const result = spawn(command, env, live);
  if (result.exitCode !== 0) {
    throw new Error(`command failed (${result.exitCode}): ${command.join(" ")}\n${result.stderr}`);
  }
  return result.stdout.trim();
}

function available(command: string): boolean {
  return spawn(["sh", "-c", `command -v ${shellQuote(command)} >/dev/null 2>&1`]).exitCode === 0;
}

function requireCommands(commands: string[]) {
  for (const command of commands) {
    if (!available(command)) throw new Error(`required command not found: ${command}`);
  }
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function sleepMs(ms: number) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function colimaStartArgs(options: Options): string[] {
  return [
    "colima", "start", options.colimaProfile,
    "--cpus", String(options.cpus),
    "--memory", String(options.memoryGiB),
    "--vm-type", "vz",
    "--mount-type", "virtiofs",
    "--runtime", "docker",
    "--activate=false",
    "--save-config=false",
  ];
}

function stopColima(options: Options) {
  spawn(["colima", "stop", options.colimaProfile]);
}

function startColima(options: Options) {
  checked(colimaStartArgs(options));
  const statusResult = spawn(["colima", "status", options.colimaProfile]);
  if (statusResult.exitCode !== 0) {
    throw new Error(`failed to read Colima status: ${statusResult.stderr}`);
  }
  const status = `${statusResult.stdout}\n${statusResult.stderr}`;
  if (!status.toLowerCase().includes("virtualization.framework") || !status.toLowerCase().includes("virtiofs")) {
    throw new Error(`Colima profile ${options.colimaProfile} is not VZ + VirtioFS. Delete only that benchmark profile and run prepare again.\n${status}`);
  }
}

function dockerContext(options: Options): string {
  const names = checked(["docker", "context", "ls", "--format", "{{.Name}}"])
    .split(/\r?\n/)
    .filter(Boolean);
  const expected = options.colimaProfile === "default" ? "colima" : `colima-${options.colimaProfile}`;
  if (names.includes(expected)) return expected;
  const fallback = names.find((name) => name.includes(options.colimaProfile));
  if (fallback) return fallback;
  throw new Error(`Docker context for Colima profile ${options.colimaProfile} was not found`);
}

function ensureAppleSystem() {
  if (spawn(["container", "system", "status"]).exitCode !== 0) {
    checked(["container", "system", "start"], {}, true);
  }
}

function machineExists(name: string): boolean {
  return spawn(["container", "machine", "inspect", name]).exitCode === 0;
}

function stopApple(options: Options) {
  if (machineExists(options.appleMachine)) spawn(["container", "machine", "stop", options.appleMachine]);
}

function startApple(options: Options) {
  checked(["container", "machine", "run", "-n", options.appleMachine, "--", "true"]);
}

function machineHome(options: Options): string {
  return checked(["container", "machine", "run", "-n", options.appleMachine, "--", "sh", "-lc", 'printf %s "$HOME"']);
}

export function resolvePathInHome(home: string, absolutePath: string): string {
  const relative = path.relative(home, absolutePath);
  if (relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative))) return relative;
  throw new Error(`${absolutePath} must be inside ${home} for Apple container machine home sharing`);
}

function machineSharedPath(options: Options, hostPath: string): string {
  const relative = resolvePathInHome(hostHome, hostPath);
  const linuxHome = machineHome(options);
  return relative === "" ? linuxHome : path.posix.join(linuxHome, ...relative.split(path.sep));
}

function defaultResultDir(): string {
  const stamp = new Date().toISOString().replaceAll(":", "-");
  return path.join(hostHome, "Library", "Caches", "dotfiles", "container-runtime-bench", "results", stamp);
}

function sharedScratch(target: string): string {
  return path.join(hostHome, "Library", "Caches", "dotfiles", "container-runtime-bench", "shared", target);
}

function processRss(kind: "colima" | "apple", snapshotPath: string): number {
  const ps = checked(["ps", "-axo", "pid=,rss=,command="]);
  const matcher = kind === "colima"
    ? /\b(colima|limactl|lima|qemu-system|vfkit)\b/i
    : /\b(container-apiserver|container-core-images|container-network-vmnet|container-runtime-linux|container-vminitd|vminitd)\b/i;
  const lines = ps.split(/\r?\n/).filter((line) => matcher.test(line));
  writeFileSync(snapshotPath, lines.join("\n") + (lines.length ? "\n" : ""));
  const kib = lines.reduce((sum, line) => {
    const match = line.trim().match(/^\d+\s+(\d+)\s+/);
    return sum + (match ? Number(match[1]) : 0);
  }, 0);
  return kib / 1024;
}

function timed(action: () => void): number {
  const start = performance.now();
  action();
  return (performance.now() - start) / 1000;
}

function runNative(options: Options): MetricRow[] {
  const scratch = sharedScratch("native");
  mkdirSync(scratch, { recursive: true });
  const output = checked(["sh", workloadPath], workloadEnv(options, "native", scratch, options.repo));
  return parseMetricCsv(output);
}

function workloadEnv(options: Options, target: string, scratch: string, repo?: string, altTarget?: string): Record<string, string> {
  const env: Record<string, string> = {
    BENCH_TARGET: target,
    BENCH_SCRATCH: scratch,
    BENCH_ITERATIONS: String(options.iterations),
    BENCH_IO_MIB: String(options.ioMiB),
    BENCH_FILE_COUNT: String(options.fileCount),
  };
  if (repo) env.BENCH_REPO = repo;
  if (altTarget) env.BENCH_ALT_TARGET = altTarget;
  return env;
}

function runColima(options: Options): MetricRow[] {
  startColima(options);
  const context = dockerContext(options);
  const scratch = sharedScratch("colima");
  mkdirSync(scratch, { recursive: true });
  const command = [
    "docker", "--context", context, "run", "--rm",
    "--mount", `type=bind,source=${scriptDir},target=/bench,readonly`,
    "--mount", `type=bind,source=${scratch},target=/scratch`,
    "--mount", "type=volume,source=runtime-bench-cargo-target,target=/local-target",
    "-e", "BENCH_TARGET=colima",
    "-e", "BENCH_SCRATCH=/scratch",
    "-e", `BENCH_ITERATIONS=${options.iterations}`,
    "-e", `BENCH_IO_MIB=${options.ioMiB}`,
    "-e", `BENCH_FILE_COUNT=${options.fileCount}`,
    "-e", "BENCH_ALT_TARGET=/local-target",
  ];
  if (options.repo) {
    command.push("--mount", `type=bind,source=${options.repo},target=/repo,readonly`, "-e", "BENCH_REPO=/repo");
  }
  command.push(options.image, "sh", "/bench/workload.sh");
  return parseMetricCsv(checked(command));
}

function runAppleHome(options: Options): MetricRow[] {
  startApple(options);
  const scratchHost = sharedScratch("apple-home");
  mkdirSync(scratchHost, { recursive: true });
  const script = machineSharedPath(options, workloadPath);
  const scratch = machineSharedPath(options, scratchHost);
  const repo = options.repo ? machineSharedPath(options, options.repo) : undefined;
  const env = workloadEnv(options, "apple-home", scratch, repo, "/var/tmp/runtime-bench-cargo-target");
  const assignments = Object.entries(env).map(([key, value]) => `${key}=${shellQuote(value)}`).join(" ");
  const command = `${assignments} sh ${shellQuote(script)}`;
  return parseMetricCsv(checked(["container", "machine", "run", "-n", options.appleMachine, "--", "sh", "-lc", command]));
}

function runAppleLocal(options: Options): MetricRow[] {
  startApple(options);
  const script = machineSharedPath(options, workloadPath);
  const scratch = "/var/tmp/container-runtime-bench/apple-local";
  let repo: string | undefined;
  if (options.repo) {
    const sharedRepo = machineSharedPath(options, options.repo);
    repo = `${scratch}/repo`;
    const setup = `rm -rf ${shellQuote(repo)} && mkdir -p ${shellQuote(repo)} && cp -a ${shellQuote(`${sharedRepo}/.`)} ${shellQuote(`${repo}/`)}`;
    checked(["container", "machine", "run", "-n", options.appleMachine, "--", "sh", "-lc", setup]);
  }
  const env = workloadEnv(options, "apple-local", scratch, repo);
  const assignments = Object.entries(env).map(([key, value]) => `${key}=${shellQuote(value)}`).join(" ");
  const command = `${assignments} sh ${shellQuote(script)}`;
  return parseMetricCsv(checked(["container", "machine", "run", "-n", options.appleMachine, "--", "sh", "-lc", command]));
}

function measureColimaOverhead(options: Options, outputDir: string): MetricRow[] {
  const rows: MetricRow[] = [];
  stopApple(options);
  for (let i = 1; i <= options.iterations; i++) {
    stopColima(options);
    sleepMs(500);
    const before = processRss("colima", path.join(outputDir, `colima-before-${i}.ps.txt`));
    const startup = timed(() => startColima(options));
    sleepMs(1000);
    const after = processRss("colima", path.join(outputDir, `colima-after-${i}.ps.txt`));
    rows.push({ target: "colima", metric: "startup_seconds", value: startup, unit: "seconds", iteration: i });
    rows.push({ target: "colima", metric: "host_rss_total_mib", value: after, unit: "MiB", iteration: i });
    rows.push({ target: "colima", metric: "host_rss_delta_mib", value: Math.max(0, after - before), unit: "MiB", iteration: i });
  }
  return rows;
}

function measureAppleOverhead(options: Options, outputDir: string): MetricRow[] {
  const rows: MetricRow[] = [];
  stopColima(options);
  for (let i = 1; i <= options.iterations; i++) {
    stopApple(options);
    sleepMs(500);
    const before = processRss("apple", path.join(outputDir, `apple-before-${i}.ps.txt`));
    const startup = timed(() => startApple(options));
    sleepMs(1000);
    const after = processRss("apple", path.join(outputDir, `apple-after-${i}.ps.txt`));
    rows.push({ target: "apple-home", metric: "startup_seconds", value: startup, unit: "seconds", iteration: i });
    rows.push({ target: "apple-home", metric: "host_rss_total_mib", value: after, unit: "MiB", iteration: i });
    rows.push({ target: "apple-home", metric: "host_rss_delta_mib", value: Math.max(0, after - before), unit: "MiB", iteration: i });
  }
  return rows;
}

export function parseMetricCsv(text: string): MetricRow[] {
  if (!text.trim()) return [];
  return text.trim().split(/\r?\n/).map((line) => {
    const [target, metric, rawValue, unit, rawIteration] = line.split(",");
    const value = Number(rawValue);
    const iteration = Number(rawIteration);
    if (!target || !metric || !unit || !Number.isFinite(value) || !Number.isInteger(iteration)) {
      throw new Error(`invalid benchmark row: ${line}`);
    }
    return { target, metric, value, unit, iteration };
  });
}

export function median(values: number[]): number {
  if (values.length === 0) throw new Error("median requires at least one value");
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function formatNumber(value: number): string {
  if (Math.abs(value) >= 1000) return value.toFixed(1);
  if (Math.abs(value) >= 10) return value.toFixed(2);
  return value.toFixed(3);
}

export function renderReport(rows: MetricRow[], metadata: Record<string, unknown>): string {
  const groups = new Map<string, MetricRow[]>();
  for (const row of rows) {
    const key = `${row.target}\u0000${row.metric}\u0000${row.unit}`;
    const group = groups.get(key) ?? [];
    group.push(row);
    groups.set(key, group);
  }

  const lines = [
    "# Container runtime benchmark",
    "",
    "## Environment",
    "",
    "```json",
    JSON.stringify(metadata, null, 2),
    "```",
    "",
    "## Results",
    "",
    "| Target | Metric | Median | Min | Max | Unit | n |",
    "|---|---|---:|---:|---:|---|---:|",
  ];

  for (const [key, group] of [...groups.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    const [target, metric, unit] = key.split("\u0000");
    const values = group.map((row) => row.value);
    lines.push(`| ${target} | ${metric} | ${formatNumber(median(values))} | ${formatNumber(Math.min(...values))} | ${formatNumber(Math.max(...values))} | ${unit} | ${values.length} |`);
  }

  lines.push(
    "",
    "`host_rss_*` は macOS の `ps` で関連プロセス名を拾った近似値です。生の process snapshot も結果ディレクトリに保存します。",
    "fio の direct I/O と portable I/O、Rust workload、実 repo の `git status` は別の性質を測るため、単一スコアには合成しません。",
    "",
  );
  return lines.join("\n");
}

function csv(rows: MetricRow[]): string {
  const lines = ["target,metric,value,unit,iteration"];
  for (const row of rows) lines.push([row.target, row.metric, row.value, row.unit, row.iteration].join(","));
  return lines.join("\n") + "\n";
}

function version(command: string[]): string | null {
  const result = spawn(command);
  if (result.exitCode !== 0) return null;
  return (result.stdout || result.stderr).trim().split(/\r?\n/)[0] ?? null;
}

function metadata(options: Options): Record<string, unknown> {
  return {
    timestamp: new Date().toISOString(),
    arch: process.arch,
    macOS: version(["sw_vers", "-productVersion"]),
    container: version(["container", "--version"]),
    colima: version(["colima", "version"]),
    docker: version(["docker", "version", "--format", "{{.Client.Version}}"]),
    rustc: version(["rustc", "--version"]),
    cpus: options.cpus,
    memoryGiB: options.memoryGiB,
    iterations: options.iterations,
    ioMiB: options.ioMiB,
    fileCount: options.fileCount,
    repo: options.repo ?? null,
    colimaProfile: options.colimaProfile,
    appleMachine: options.appleMachine,
    image: options.image,
  };
}

function prepare(options: Options) {
  if (process.platform !== "darwin") throw new Error("container runtime benchmark preparation must run on macOS");
  requireCommands(["colima", "container", "docker"]);
  ensureAppleSystem();

  startColima(options);
  const context = dockerContext(options);
  checked(["docker", "--context", context, "build", "-t", options.image, scriptDir], {}, true);
  checked(["container", "build", "-t", options.image, scriptDir], {}, true);

  if (options.recreate && machineExists(options.appleMachine)) {
    stopApple(options);
    checked(["container", "machine", "rm", options.appleMachine], {}, true);
  }
  if (!machineExists(options.appleMachine)) {
    checked([
      "container", "machine", "create", options.image,
      "--name", options.appleMachine,
      "--cpus", String(options.cpus),
      "--memory", `${options.memoryGiB}G`,
      "--home-mount", "rw",
    ], {}, true);
  } else {
    checked([
      "container", "machine", "set", "-n", options.appleMachine,
      `cpus=${options.cpus}`,
      `memory=${options.memoryGiB}G`,
      "home-mount=rw",
    ], {}, true);
  }

  stopApple(options);
  stopColima(options);
  console.log("Prepared dedicated benchmark runtimes. Run: mise run container-runtime:bench");
}

function runBenchmark(options: Options) {
  if (process.platform !== "darwin") throw new Error("container runtime benchmark must run on macOS");
  requireCommands(["colima", "container", "docker", "rustc", "cargo"]);
  ensureAppleSystem();
  if (!machineExists(options.appleMachine)) throw new Error("Apple benchmark machine is missing; run prepare first");
  if (options.repo) resolvePathInHome(hostHome, options.repo);

  const outputDir = options.output ?? defaultResultDir();
  mkdirSync(outputDir, { recursive: true });
  const rows: MetricRow[] = [];

  stopApple(options);
  stopColima(options);
  rows.push(...runNative(options));

  rows.push(...measureColimaOverhead(options, outputDir));
  rows.push(...runColima(options));
  stopColima(options);

  rows.push(...measureAppleOverhead(options, outputDir));
  rows.push(...runAppleHome(options));
  rows.push(...runAppleLocal(options));
  stopApple(options);

  const meta = metadata(options);
  writeFileSync(path.join(outputDir, "results.csv"), csv(rows));
  writeFileSync(path.join(outputDir, "metadata.json"), JSON.stringify(meta, null, 2) + "\n");
  writeFileSync(path.join(outputDir, "report.md"), renderReport(rows, meta));
  console.log(path.join(outputDir, "report.md"));
}

if (import.meta.main) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.command === "prepare") prepare(options);
    else runBenchmark(options);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
