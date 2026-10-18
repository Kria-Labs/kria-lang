<p align="center">
  <img src="kria-ico.png" alt="Kria" width="140">
</p>

<h1 align="center">Kria</h1>

<p align="center">
  A small, fast programming language — Rust bytecode VM, <code>.krx</code> sources.
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#language-overview">Language</a> ·
  <a href="#examples">Examples</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="#license">License</a>
</p>

---

## Quick start

**Requirements:** [Rust](https://rustup.rs/) (2021 edition)

```bash
git clone <repo-url> && cd kria-lang
cargo build --release
```

| Command | What it does |
|---------|----------------|
| `kria` | Interactive REPL |
| `kria program.krx` | Run entry file (+ imported modules) |
| `kria -h` | Help |

```bash
./target/release/kria test.krx
./target/release/kria examples/13_imports/main.krx
```

**REPL:** persistent session, auto-print for expressions, multi-line blocks, `:help` / `:reset` / `:exit`.

---

## Language overview

### Core

- Dynamic typing · `set` bindings · `print()` · `type()` · `wait(ms)` · `//` and `/* */` comments · `"..."` / `"""..."""` strings · newline-terminated statements
- Operators: arithmetic, comparisons, `and` / `or` / `not`
- Control flow: `if` / `elseif` / `else`, `while`, `break`, `continue`
- Functions: `fn name(...)`, lambdas `fn(...) { }`, `return`, closures with captured variables

### Data

| Feature | Summary |
|---------|---------|
| **Arrays** | Mutable `[...]`, immutable `#[...]`, index, `.length`, `push` / `pop`, `for item in arr` |
| **Objects** | `{ key: value }`, dot / bracket access, property assign, `rmv()`, deep equality, `for key, value in obj` |
| **Integer and float** | 64-bit integers and double-precision floating-point values; mixed arithmetic promotes to float |
| **Input** | `input<str>`, `input<int>`, and `input<float>` with prompts |

Decimal and scientific-notation literals are supported (`0.25`, `.5`, `2e3`).
Integer-only division remains integer division; an operation with a float
produces a floating-point result.

### Modules (multi-file)

Run **one entry file**; other `.krx` files load only via `import`:

```kria
import math from "./math.krx"
print(math.add(2, 3))
```

- `export fn` — visible to importers; other functions stay private
- Relative paths only (`./`, `../`); circular imports are rejected
- Package manager (KPM) is planned later

---

## Examples

Runnable samples live under [`examples/`](examples/):

| File | Topic |
|------|--------|
| `01_basics.krx` | Variables, print, operators |
| `02_conditionals.krx` | `if` / `elseif` |
| `03_loops.krx` | `while`, `for-in` |
| `04_functions_basic.krx` | Functions, closures |
| `11_arrays.krx` | Arrays |
| `12_objects.krx` | Objects |
| `13_imports/main.krx` | Multi-file imports |
| `14_type_wait.krx` | `type()`, `wait()` |
| `15_comments_strings.krx` | Block comments, multiline strings |

---

## Architecture

```text
Source (.krx) → Lexer → Parser → Compiler → Bytecode → Stack VM
```

- Flat bytecode, constant pool, call frames for functions
- Hot-loop optimizations for global and local counters, plus specialized global string appends
- Monomorphic object-property caches, shared closure environments, and IR/bytecode cleanup passes
- Implementation: [`src/`](src/) (`lexer`, `parser`, `compiler`, `vm`, `repl`, `modules`, `project`)

---

## Benchmarks

The benchmark suite requires Node.js 16.6+ and compares equivalent Kria,
Node.js, and Python implementations for numeric work, control flow, calls,
closures, collections, and strings at two workload sizes. Python is optional;
missing runtimes are reported as skipped. Workload order is randomized,
every run's exit status and integer result are validated, and raw samples plus
a JSON summary are saved under `benchmarks/results/`.
The primary workload timing is measured in-process to exclude process startup:
Node.js and Python time the workload directly, while a small Rust harness times
Kria VM execution. Kria source compilation, end-to-end process time, and a
minimal startup baseline are reported separately.

```bash
node benchmarks/run.mjs
# or: ./benchmarks/benchmarks.sh
```

On Windows PowerShell, run `.\benchmarks\benchmarks.ps1`.
The runner builds Kria in release mode unless `--no-build` is provided.

By default the runner uses 2 warmups and 7 measured runs. For a quick check:

```bash
node benchmarks/run.mjs --sizes small --warmup 0 --runs 1
```

Options include `--sizes small,large`, `--languages kria,node,python`,
`--warmup N`, `--runs N`, and `--seed N`. Set `BENCH_PYTHON` to select a Python
interpreter, `BENCH_KRIA` to choose the Kria startup-baseline executable, or
`BENCH_KRIA_BENCH` to choose the timed Kria harness. Results include median,
p90, median absolute deviation (MAD), raw per-run timings, compile/process
times, runtime versions, host details, and binary/suite SHA-256 hashes. A
speedup greater than `1x` means that runtime executed the same workload faster
than Kria; unlike workloads are not averaged into one score. Process startup is
visible in the separate process-time column and startup-baseline row.

---

## Install (binary)

**Linux / macOS** — from the repo root (or any directory):

```bash
./release/build.sh install    # build + copy to ~/.kria/bin
```

`install` updates `~/.bashrc` / `~/.zshrc` when needed. Open a **new terminal** or run `source ~/.bashrc`, then `kria --help`.

If `kria` is still unknown in the current window:

```bash
export PATH="${PATH}:$HOME/.kria/bin"
```

Other commands: `./release/build.sh` (build only), `./release/build.sh package` (tarball in `release/`).

**Windows** — `cargo build --release`, then NSIS via `release/kria-setup.nsi`.

---

## Contributing

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for setup, code style, and pull request guidelines.

---

## License

[MIT](LICENSE) — Copyright (c) 2026 Piotriox
