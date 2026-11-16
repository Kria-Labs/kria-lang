<p align="center">
  <img src="kria-ico.png" alt="Kria" width="140">
</p>

<h1 align="center">Kria</h1>

<p align="center">
  A small, fast programming language — Rust Register VM, <code>.krx</code> sources.
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#language-overview">Language</a> ·
  <a href="#examples">Examples</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#benchmarks">Benchmarks</a> ·
  <a href="#install-binary">Install</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="#license">License</a>
</p>

---

## Quick start

**Requirements:** [Rust](https://rustup.rs/) (2021 edition) to build. Running
source files with the default JIT also needs a native toolchain — see
[Platform requirements](#platform-requirements).

```bash
git clone <repo-url> && cd kria-lang
cargo build --release
```

| Command | What it does |
|---------|----------------|
| `kria` | Interactive REPL |
| `kria program.krx` | Run entry file (+ imported modules) |
| `kria -h` | Help |
| `kria -v` | Print Kria version |

```bash
./target/release/kria test.krx
./target/release/kria examples/13_imports/main.krx
```

Source files use the experimental native LLVM-based JIT compiler by default.
Set `KRIA_BACKEND=register` to use the Register VM instead:

```bash
# macOS / Linux (Register VM)
KRIA_BACKEND=register ./target/release/kria program.krx

# Windows PowerShell
$env:KRIA_BACKEND="register"
.\target\release\kria.exe program.krx
```

The JIT compiles generated LLVM IR to a platform-native shared library
(`.dll`, `.dylib`, or `.so`) by running `clang`, then loads it through a
cross-platform dynamic-library API. Compiled artifacts are cached in the
operating system's temporary directory, keyed by the generated IR, clang
version and target, and JIT optimization level.

Remove the environment variable with `unset KRIA_BACKEND` on macOS/Linux or
`Remove-Item Env:KRIA_BACKEND` in PowerShell to return to the default JIT.
The backend setting applies to source-file execution; the interactive REPL
always uses the Register VM, regardless of `KRIA_BACKEND`.

### Platform requirements

The REPL and the Register VM (`KRIA_BACKEND=register`) have no requirements
beyond the `kria` binary. The default JIT backend calls `clang` at run time to
compile and link native code, so the machine running `.krx` files needs a
working C toolchain. Release packages and the Windows installer do **not**
bundle this toolchain.

| Platform | Needed to run with the JIT | Install |
|----------|----------------------------|---------|
| **Windows** | Clang (LLVM) on `PATH`, plus the MSVC linker, MSVC C runtime libraries, and Windows SDK | [LLVM for Windows](https://github.com/llvm/llvm-project/releases) (tick *Add LLVM to the system PATH*) and [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (MSVC v14x + Windows SDK) |
| **Linux** | `clang`, a system linker (`ld` from binutils), and C runtime/development files (glibc and libgcc) | Debian/Ubuntu: `sudo apt install clang` · Fedora: `sudo dnf install clang` · Arch: `sudo pacman -S clang` |
| **macOS** | Xcode Command Line Tools (provide `clang`, the linker, and the macOS SDK) | `xcode-select --install` |

Notes:

- **Windows:** Clang uses the MSVC toolchain to link the generated DLL. It
  finds Build Tools automatically, so a *Developer Command Prompt* is not
  required. The `kria.exe` binary itself also needs the Microsoft Visual C++
  Redistributable (`VCRUNTIME140.dll`), which Build Tools installs.
- **Linux:** package managers install binutils, glibc headers, and libgcc
  together with `clang`. On minimal images, also install `build-essential`
  (Debian/Ubuntu) or `gcc` (Fedora).
- Check the setup with `clang --version`. If the toolchain is missing,
  source files fail to run under the JIT with a `Failed to execute clang` or
  link error; set `KRIA_BACKEND=register` to run them on the Register VM.

**REPL:** persistent Register VM session, auto-print for expressions, multi-line blocks, `:help` / `:reset` / `:exit`.

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

- `export fn` — visible to importers, only through the import alias (`math.add`)
- Each file has its own namespace: top-level variables and non-exported
  functions are private to the file, so names never clash between modules
- A module imported from several files is loaded and run once
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
Source (.krx) → Lexer → Parser → SSA/module → Register VM
                                      └→ LLVM JIT (experimental)
```

- LLVM JIT is the default for source-file execution from the CLI; set
  `KRIA_BACKEND=register` to use the Register VM
- REPL uses the Register VM with persistent globals and functions
- SSA register instructions, global/local/upvalue storage, call frames, and closures
- LLVM JIT is experimental and requires a native Clang toolchain at run time
  (see [Platform requirements](#platform-requirements))
- Hot-loop optimizations for global and local counters, plus specialized global string appends
- Monomorphic object-property caches and shared closure environments
- Implementation: [`src/`](src/) (`lexer`, `parser`, `register`, `vm`, `jit`,
  `repl`, `modules`, `project`)

---

## Benchmarks

The benchmark suite requires Node.js 16.6+ and compares equivalent Register VM,
JIT, Node.js, and Python implementations for numeric work, control flow, calls,
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

Options include `--sizes small,large`, `--languages register,jit,node,python`,
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

Installed binaries need the toolchain from
[Platform requirements](#platform-requirements) to run source files with the
default JIT; without it, set `KRIA_BACKEND=register`.

---

## Contributing

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for setup, code style, and pull request guidelines.

---

## License

[MIT](LICENSE) — Copyright (c) 2026 Piotriox
