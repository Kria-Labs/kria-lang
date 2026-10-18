import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpus, platform, release, totalmem, arch, tmpdir, homedir } from "node:os";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";

const scriptPath = fileURLToPath(import.meta.url);
const benchmarkDir = dirname(scriptPath);
const projectRoot = resolve(benchmarkDir, "..");
const manifest = JSON.parse(readFileSync(join(benchmarkDir, "suite.json"), "utf8"));
const defaults = { warmup: 2, runs: 7, sizes: ["small", "large"], languages: ["kria", "node", "python"] };

function parseArguments(args) {
  const options = { ...defaults, build: true, seed: 20261018, output: null };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    const value = args[i + 1];
    if (arg === "--no-build") {
      options.build = false;
    } else if (["--warmup", "--runs", "--sizes", "--languages", "--seed", "--output"].includes(arg)) {
      if (value === undefined) throw new Error(`Missing value for ${arg}`);
      i++;
      if (arg === "--warmup") options.warmup = Number(value);
      if (arg === "--runs") options.runs = Number(value);
      if (arg === "--sizes") options.sizes = value.split(",");
      if (arg === "--languages") options.languages = value.split(",");
      if (arg === "--seed") options.seed = Number(value);
      if (arg === "--output") options.output = resolve(value);
    } else if (arg === "--help" || arg === "-h") {
      console.log(`Usage: node benchmarks/run.mjs [options]

  --warmup N                 Warmup runs per workload/runtime (default ${defaults.warmup})
  --runs N                   Measured runs (default ${defaults.runs})
  --sizes small,large        Workload sizes (default small,large)
  --languages kria,node,python  Runtimes to compare
  --seed N                   Deterministic run-order shuffle seed
  --output DIR               Result directory (default benchmarks/results/<timestamp>)
  --no-build                 Do not run cargo build --release`);
      process.exit(0);
    } else {
      throw new Error(`Unknown option: ${arg}`);
    }
  }
  if (!Number.isInteger(options.warmup) || options.warmup < 0) throw new Error("--warmup must be a non-negative integer");
  if (!Number.isInteger(options.runs) || options.runs < 1) throw new Error("--runs must be a positive integer");
  if (!Number.isInteger(options.seed)) throw new Error("--seed must be an integer");
  if (options.sizes.length === 0 || options.sizes.some((size) => !["small", "large"].includes(size))) {
    throw new Error("--sizes must contain small and/or large");
  }
  if (options.languages.length === 0 || options.languages.some((language) => !["kria", "node", "python"].includes(language))) {
    throw new Error("--languages must contain kria, node, and/or python");
  }
  return options;
}

function runProcess(command, args, options = {}) {
  const started = process.hrtime.bigint();
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? projectRoot,
    encoding: "utf8",
    timeout: options.timeout ?? 120_000,
    maxBuffer: 4 * 1024 * 1024,
    windowsHide: true,
    env: process.env,
  });
  const elapsedNs = process.hrtime.bigint() - started;
  return { ...result, elapsedNs };
}

function findOnPath(command) {
  const extensions = process.platform === "win32"
    ? (process.env.PATHEXT ?? ".EXE;.CMD;.BAT").split(";")
    : [""];
  for (const directory of (process.env.PATH ?? "").split(process.platform === "win32" ? ";" : ":")) {
    if (!directory) continue;
    for (const extension of extensions) {
      const candidate = join(directory, command + (extname(command) ? "" : extension));
      if (!existsSync(candidate)) continue;
      return candidate;
    }
  }
  return null;
}

function findPython() {
  if (process.env.BENCH_PYTHON) {
    const executable = process.env.BENCH_PYTHON.replace(/^"(.*)"$/, "$1");
    return { command: executable, args: [] };
  }
  const candidates = process.platform === "win32"
    ? [{ name: "python", args: [] }, { name: "python3", args: [] }, { name: "py", args: ["-3"] }]
    : [{ name: "python3", args: [] }, { name: "python", args: [] }];
  const failures = [];
  for (const candidate of candidates) {
    const command = findOnPath(candidate.name) ?? candidate.name;
    const version = runProcess(command, [...candidate.args, "--version"], { timeout: 10_000 });
    if (version.status === 0) return { command, args: candidate.args, version: (version.stdout + version.stderr).trim() };
    const details = (version.stderr || version.stdout || version.error?.message || `exit ${version.status}`).trim();
    failures.push(`${command}: ${details}`);
  }
  if (process.platform === "win32") {
    const localAppData = process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local");
    const managedPythonRoot = join(localAppData, "Python");
    try {
      const installs = readdirSync(managedPythonRoot, { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && /^pythoncore-/i.test(entry.name))
        .map((entry) => join(managedPythonRoot, entry.name, "python.exe"))
        .filter((executable) => existsSync(executable));
      for (const executable of installs) {
        const version = runProcess(executable, ["--version"], { timeout: 10_000 });
        if (version.status === 0) {
          return { command: executable, args: [], version: (version.stdout + version.stderr).trim() };
        }
        const details = (version.stderr || version.stdout || version.error?.message || `exit ${version.status}`).trim();
        failures.push(`${executable}: ${details}`);
      }
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  return {
    reason: failures.length
      ? `no working Python runtime found (${failures.join("; ")}). Install Python or set BENCH_PYTHON to its executable.`
      : "no Python launcher found on PATH. Install Python or set BENCH_PYTHON to its executable.",
  };
}

function getGitRevision() {
  const result = runProcess("git", ["rev-parse", "--short", "HEAD"], { timeout: 10_000 });
  return result.status === 0 ? result.stdout.trim() : null;
}

function getToolVersion(command, args) {
  const result = runProcess(command, args, { timeout: 10_000 });
  return result.status === 0 ? (result.stdout + result.stderr).trim() : null;
}

function benchmarkSourcesHash(workloads) {
  const files = [
    join(benchmarkDir, "suite.json"),
    join(benchmarkDir, "implementations", "node.js"),
    join(benchmarkDir, "implementations", "python.py"),
    join(projectRoot, "src", "bin", "kria_bench.rs"),
    ...workloads.map((workload) => join(benchmarkDir, "kria", `${workload.id}.krx.in`)),
  ].sort();
  const hash = createHash("sha256");
  for (const file of files) {
    hash.update(relative(benchmarkDir, file).split(sep).join("/"));
    hash.update(readFileSync(file));
  }
  return hash.digest("hex");
}

function runtimeInfo(name, executable, args = []) {
  const version = runProcess(executable, [...args, "--version"], { timeout: 10_000 });
  if (version.status !== 0) throw new Error(`${name} version probe failed: ${(version.stderr || version.stdout).trim()}`);
  return { name, executable, args, version: (version.stdout + version.stderr).trim() };
}

function resolveRuntimes(options) {
  const selected = new Set(options.languages);
  const runtimes = [];
  const missing = [];
  const kriaBinary = process.env.BENCH_KRIA ?? join(projectRoot, "target", "release", process.platform === "win32" ? "kria.exe" : "kria");
  const kriaBenchmarkBinary = process.env.BENCH_KRIA_BENCH
    ?? join(projectRoot, "target", "release", process.platform === "win32" ? "kria_bench.exe" : "kria_bench");

  if (selected.has("kria")) {
    if (options.build && !process.env.BENCH_KRIA_BENCH) {
      console.log("Building Kria release binary...");
      const build = runProcess("cargo", ["build", "--release"], { timeout: 600_000 });
      if (build.status !== 0) throw new Error(`cargo build --release failed:\n${build.stdout}${build.stderr}`);
    }
    if (!existsSync(kriaBinary)) throw new Error(`Kria binary not found: ${kriaBinary}`);
    if (!existsSync(kriaBenchmarkBinary)) throw new Error(`Kria benchmark runner not found: ${kriaBenchmarkBinary}`);
    runtimes.push({
      ...runtimeInfo("kria", kriaBinary),
      executionExecutable: kriaBenchmarkBinary,
      startupExecutable: kriaBinary,
    });
  }
  if (selected.has("node")) runtimes.push(runtimeInfo("node", process.execPath));
  if (selected.has("python")) {
    const python = findPython();
    if (python.command) {
      const info = runtimeInfo("python", python.command, python.args);
      runtimes.push({ ...info, args: python.args });
    } else {
      missing.push(`python: ${python.reason}`);
    }
  }
  for (const name of selected) {
    if (!runtimes.some((runtime) => runtime.name === name) && !missing.some((item) => item.startsWith(`${name}:`))) {
      missing.push(`${name}: runtime unavailable`);
    }
  }
  return { runtimes, missing };
}

function normalizeResult(stdout, label) {
  const output = stdout.trim();
  if (!/^-?\d+$/.test(output)) throw new Error(`${label}: expected one integer result, got ${JSON.stringify(output)}`);
  return BigInt(output).toString();
}

function invoke(runtime, workload, iterations, kriaFile) {
  if (runtime.name === "kria") {
    const result = runProcess(
      workload === "startup" ? runtime.startupExecutable : runtime.executionExecutable,
      [kriaFile],
    );
    if (workload !== "startup") {
      result.executionNs = readTimingMarker(result.stderr, "BENCH_EXECUTION_NS");
      result.compileNs = readTimingMarker(result.stderr, "BENCH_COMPILE_NS");
    }
    return result;
  }
  if (runtime.name === "node") {
    if (workload === "startup") return runProcess(runtime.executable, ["-e", "console.log(1)"]);
    const result = runProcess(runtime.executable, [join(benchmarkDir, "implementations", "node.js"), workload, String(iterations)]);
    result.executionNs = readTimingMarker(result.stderr, "BENCH_EXECUTION_NS");
    return result;
  }
  if (workload === "startup") return runProcess(runtime.executable, [...runtime.args, "-c", "print(1)"]);
  const result = runProcess(runtime.executable, [...runtime.args, join(benchmarkDir, "implementations", "python.py"), workload, String(iterations)]);
  result.executionNs = readTimingMarker(result.stderr, "BENCH_EXECUTION_NS");
  return result;
}

function readTimingMarker(stderr, marker) {
  const match = stderr.match(new RegExp(`^${marker}=(\\d+)$`, "m"));
  return match ? BigInt(match[1]) : null;
}

function validateExecution(result, label, expected, requireExecutionTimer = false) {
  if (result.error) throw new Error(`${label}: ${result.error.message}`);
  if (result.status !== 0) {
    const details = (result.stderr || result.stdout).trim();
    throw new Error(`${label}: exited with ${result.status ?? "signal"}${details ? `: ${details}` : ""}`);
  }
  if (requireExecutionTimer && (result.executionNs === null || result.compileNs === null)) {
    throw new Error(`${label}: required execution or compilation timer marker was missing`);
  }
  const actual = normalizeResult(result.stdout, label);
  if (expected !== undefined && actual !== expected) {
    throw new Error(`${label}: output ${actual} did not match expected ${expected}`);
  }
  return actual;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function summarize(samples) {
  const values = samples.map((sample) => sample.elapsed_ns / 1e6);
  const processValues = samples.map((sample) => Number(sample.process_elapsed_ns) / 1e6);
  const compileValues = samples
    .filter((sample) => sample.compile_ns !== null)
    .map((sample) => Number(sample.compile_ns) / 1e6);
  const center = median(values);
  const sorted = [...values].sort((a, b) => a - b);
  return {
    samples: values.length,
    median_ms: center,
    p90_ms: sorted[Math.max(0, Math.ceil(sorted.length * 0.9) - 1)],
    mad_ms: median(values.map((value) => Math.abs(value - center))),
    min_ms: sorted[0],
    max_ms: sorted.at(-1),
    mean_ms: values.reduce((sum, value) => sum + value, 0) / values.length,
    process_median_ms: median(processValues),
    compile_median_ms: compileValues.length ? median(compileValues) : null,
  };
}

function shuffle(items, random) {
  const shuffled = [...items];
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  return shuffled;
}

function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

function timestamp() {
  return new Date().toISOString().replaceAll(":", "-");
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const startedAt = new Date().toISOString();
  const { runtimes, missing } = resolveRuntimes(options);
  if (runtimes.length === 0) throw new Error(`No benchmark runtimes are available${missing.length ? `:\n${missing.join("\n")}` : ""}`);
  for (const message of missing) console.warn(`SKIP ${message}`);

  const selectedWorkloads = manifest.workloads.filter((workload) => options.sizes.some((size) => size in workload.sizes));
  if (selectedWorkloads.length === 0) throw new Error("No workloads selected");
  const resultDir = options.output ?? join(benchmarkDir, "results", timestamp());
  mkdirSync(resultDir, { recursive: true });
  const rawPath = join(resultDir, "samples.jsonl");
  const summaryPath = join(resultDir, "summary.json");
  writeFileSync(rawPath, "");

  const tempDir = mkdtempSync(join(tmpdir(), "kria-bench-"));
  const combos = [];
  const errors = [];
  const outputs = new Map();
  const templates = new Map();
  try {
    for (const workload of selectedWorkloads) {
      for (const size of options.sizes) {
        const iterations = workload.sizes[size];
        if (iterations === undefined) continue;
        let kriaFile = null;
        if (runtimes.some((runtime) => runtime.name === "kria")) {
          const templatePath = join(benchmarkDir, "kria", `${workload.id}.krx.in`);
          let template = templates.get(templatePath);
          if (template === undefined) {
            template = readFileSync(templatePath, "utf8");
            templates.set(templatePath, template);
          }
          const source = template.replaceAll("{{N}}", String(iterations));
          if (source.includes("{{")) throw new Error(`Unresolved template token in ${templatePath}`);
          kriaFile = join(tempDir, `${workload.id}-${size}.krx`);
          writeFileSync(kriaFile, source);
        }
        for (const runtime of runtimes) combos.push({ workload, size, iterations, kriaFile, runtime });
      }
    }
    const startupFile = join(tempDir, "startup.krx");
    writeFileSync(startupFile, "print(1)\n");
    for (const runtime of runtimes) {
      combos.push({
        workload: { id: "startup", category: "startup" },
        size: "baseline",
        iterations: 0,
        kriaFile: startupFile,
        runtime,
      });
    }

    console.log(`Benchmark suite: ${selectedWorkloads.length} workloads, ${options.sizes.join(", ")} sizes, ${runtimes.map((item) => item.name).join(", ")}`);
    console.log("Measurement: workloads are timed in-process; Kria compilation and total process time are recorded separately.");
    console.log(`Warmup: ${options.warmup}; measured runs: ${options.runs}; seed: ${options.seed}`);
    console.log(`Results: ${resultDir}`);

    const baseline = new Map();
    for (const combo of combos) {
      const key = `${combo.workload.id}:${combo.size}`;
      const label = `${combo.runtime.name}/${key}`;
      try {
        const actual = validateExecution(
          invoke(combo.runtime, combo.workload.id, combo.iterations, combo.kriaFile),
          label,
          undefined,
          combo.workload.id !== "startup",
        );
        if (!baseline.has(key)) baseline.set(key, actual);
        if (actual !== baseline.get(key)) throw new Error(`${label}: result ${actual} differs from ${baseline.get(key)} in the equivalent implementations`);
        outputs.set(`${combo.runtime.name}:${key}`, actual);
      } catch (error) {
        errors.push({ workload: combo.workload.id, size: combo.size, runtime: combo.runtime.name, phase: "validation", message: error.message });
      }
    }

    const validCombos = combos.filter((combo) => outputs.has(`${combo.runtime.name}:${combo.workload.id}:${combo.size}`));
    for (const combo of validCombos) {
      const expected = outputs.get(`${combo.runtime.name}:${combo.workload.id}:${combo.size}`);
      for (let i = 0; i < options.warmup; i++) {
        try {
          validateExecution(
            invoke(combo.runtime, combo.workload.id, combo.iterations, combo.kriaFile),
            `${combo.runtime.name}/${combo.workload.id}/${combo.size} warmup`,
            expected,
            combo.workload.id !== "startup",
          );
        } catch (error) {
          errors.push({ workload: combo.workload.id, size: combo.size, runtime: combo.runtime.name, phase: "warmup", message: error.message });
        }
      }
    }

    const samplesByCombo = new Map(validCombos.map((combo) => [combo, []]));
    const random = seededRandom(options.seed);
    for (let run = 0; run < options.runs; run++) {
      for (const combo of shuffle(validCombos, random)) {
        const key = `${combo.runtime.name}:${combo.workload.id}:${combo.size}`;
        const label = `${combo.runtime.name}/${combo.workload.id}/${combo.size} run ${run + 1}`;
        try {
          const result = invoke(combo.runtime, combo.workload.id, combo.iterations, combo.kriaFile);
          validateExecution(result, label, outputs.get(key), combo.workload.id !== "startup");
          const sample = {
            run: run + 1,
            runtime: combo.runtime.name,
            workload: combo.workload.id,
            category: combo.workload.category,
            size: combo.size,
            iterations: combo.iterations,
            elapsed_ns: (combo.workload.id === "startup" ? result.elapsedNs : result.executionNs).toString(),
            process_elapsed_ns: result.elapsedNs.toString(),
            compile_ns: result.compileNs?.toString() ?? null,
            output: outputs.get(key),
          };
          samplesByCombo.get(combo).push(sample);
          appendFileSync(rawPath, `${JSON.stringify(sample)}\n`);
        } catch (error) {
          errors.push({ workload: combo.workload.id, size: combo.size, runtime: combo.runtime.name, phase: `run ${run + 1}`, message: error.message });
        }
      }
    }

    const summaries = [];
    for (const combo of validCombos) {
      const samples = samplesByCombo.get(combo);
      const row = {
        workload: combo.workload.id,
        category: combo.workload.category,
        size: combo.size,
        iterations: combo.iterations,
        runtime: combo.runtime.name,
        output: outputs.get(`${combo.runtime.name}:${combo.workload.id}:${combo.size}`),
        ...(samples.length === options.runs ? summarize(samples) : { error: `Only ${samples.length}/${options.runs} samples completed` }),
      };
      summaries.push(row);
    }
    for (const row of summaries) {
      const reference = summaries.find((other) => other.workload === row.workload && other.size === row.size && other.runtime === "kria");
      if (row.runtime === "kria" || row.workload === "startup") {
        row.speedup_vs_kria = null;
        continue;
      }
      if (!reference || reference.error || row.error || row.median_ms <= 0) {
        row.speedup_vs_kria = null;
        row.comparison_note = "reference unavailable";
      } else {
        row.speedup_vs_kria = Number((reference.median_ms / row.median_ms).toFixed(3));
        row.comparison_note = "execution-time ratio";
      }
    }

    const metadata = {
      schema_version: 1,
      started_at: startedAt,
      revision: getGitRevision(),
      benchmark_suite_sha256: benchmarkSourcesHash(selectedWorkloads),
      host: {
        platform: `${platform()} ${release()}`,
        architecture: arch(),
        cpu: cpus()[0]?.model ?? "unknown",
        logical_cpus: cpus().length,
        total_memory_bytes: totalmem(),
      },
      measurement: "in-process workload execution; process startup excluded; Kria source compilation reported separately",
      process_measurement: "end-to-end process wall time, recorded separately for each workload sample",
      timing_clock: "process.hrtime.bigint, performance_counter_ns, or Rust Instant per runtime",
      warmup_runs: options.warmup,
      measured_runs: options.runs,
      order_seed: options.seed,
      runtimes: runtimes.map(({ name, version }) => ({ name, version })),
      rust_toolchain: runtimes.some((runtime) => runtime.name === "kria")
        ? { cargo: getToolVersion("cargo", ["-V"]), rustc: getToolVersion("rustc", ["-V"]) }
        : null,
      skipped_runtimes: missing,
      kria_binary_sha256: runtimes.find((runtime) => runtime.name === "kria")
        ? createHash("sha256").update(readFileSync(runtimes.find((runtime) => runtime.name === "kria").executable)).digest("hex")
        : null,
      kria_benchmark_runner_sha256: runtimes.find((runtime) => runtime.name === "kria")
        ? createHash("sha256").update(readFileSync(runtimes.find((runtime) => runtime.name === "kria").executionExecutable)).digest("hex")
        : null,
      workloads: selectedWorkloads,
      results: summaries,
      errors,
    };
    writeFileSync(summaryPath, `${JSON.stringify(metadata, null, 2)}\n`);

    console.log("\nWorkload             Size     Runtime   Exec med(ms)  P90(ms)  MAD(ms)  Compile(ms)  Process(ms)  Speedup vs Kria");
    console.log("-".repeat(126));
    for (const row of summaries) {
      const medianText = row.median_ms === undefined ? "ERROR" : row.median_ms.toFixed(3);
      const p90Text = row.p90_ms === undefined ? "-" : row.p90_ms.toFixed(3);
      const madText = row.mad_ms === undefined ? "-" : row.mad_ms.toFixed(3);
      const speedupText = row.speedup_vs_kria === null ? "-" : `${row.speedup_vs_kria}x`;
      const compileText = row.compile_median_ms === null || row.compile_median_ms === undefined ? "-" : row.compile_median_ms.toFixed(3);
      const processText = row.process_median_ms === undefined ? "-" : row.process_median_ms.toFixed(3);
      console.log(`${row.workload.padEnd(20)} ${row.size.padEnd(8)} ${row.runtime.padEnd(9)} ${medianText.padStart(12)} ${p90Text.padStart(8)} ${madText.padStart(8)} ${compileText.padStart(12)} ${processText.padStart(12)} ${speedupText.padStart(16)}`);
    }
    if (errors.length) {
      console.error(`\n${errors.length} benchmark error(s):`);
      for (const error of errors) console.error(`- ${error.runtime}/${error.workload}/${error.size} (${error.phase}): ${error.message}`);
    }
    console.log(`\nRaw samples: ${rawPath}`);
    console.log(`Summary: ${summaryPath}`);
    if (errors.length || summaries.some((row) => row.error)) process.exitCode = 1;
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
