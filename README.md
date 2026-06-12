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

- Dynamic typing · `set` bindings · `print()` · `type()` · `wait(ms)` · `//` comments · newline-terminated statements
- Operators: arithmetic, comparisons, `and` / `or` / `not`
- Control flow: `if` / `elseif` / `else`, `while`, `break`, `continue`
- Functions: `fn name(...)`, lambdas `fn(...) { }`, `return`, closures (copy-on-create captures)

### Data

| Feature | Summary |
|---------|---------|
| **Arrays** | Mutable `[...]`, immutable `#[...]`, index, `.length`, `push` / `pop`, `for item in arr` |
| **Objects** | `{ key: value }`, dot / bracket access, property assign, `rmv()`, deep equality, `for key, value in obj` |
| **Input** | `input<str>`, `input<int>`, `input<float>` with prompts |

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

---

## Architecture

```text
Source (.krx) → Lexer → Parser → Compiler → Bytecode → Stack VM
```

- Flat bytecode, constant pool, call frames for functions
- Hot-loop optimizations (`OP_LOOP_INC_LESS`, combined global ops)
- Implementation: [`src/`](src/) (`lexer`, `parser`, `compiler`, `vm`, `repl`, `modules`, `project`)

---

## Benchmarks

```bash
cargo build --release
./benchmarks/benchmarks.sh
```

Results are written to [`benchmarks/benchmark_results.txt`](benchmarks/benchmark_results.txt). Optional: [hyperfine](https://github.com/sharkdp/hyperfine) for stabler timings. Override runs with `BENCH_WARMUP=3 BENCH_RUNS=10`.

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
