# Contributing to Kria

Thank you for your interest in Kria. This document explains how to get set up, make changes, and open pull requests.

## Getting started

1. **Fork** the repository and clone your fork.
2. Install a recent **stable Rust** toolchain ([rustup](https://rustup.rs/)).
3. Build and run tests:

```bash
cargo build
cargo test
cargo test --all-targets
cargo build --release
./target/release/kria test.krx
```

On Windows, run the built executable as `.\target\release\kria.exe test.krx`.

Run the following checks before submitting a change:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

4. Explore [`examples/`](examples/) to see language features in context.

## Project layout

| Path | Purpose |
|------|---------|
| `src/lexer.rs` | Tokenization |
| `src/parser.rs` | AST / syntax |
| `src/compiler.rs` | Stack bytecode generation and compilation pipeline |
| `src/ir/` | IR lifting, optimization, SSA utilities, and bytecode emission |
| `src/register.rs` | Register/SSA lowering and Register VM instructions |
| `src/optimizer.rs` | Bytecode peephole optimization |
| `src/bytecode.rs` | Legacy stack bytecode format and instruction helpers |
| `src/vm.rs` | Stack VM and Register VM implementations |
| `src/repl.rs` | Interactive REPL |
| `src/modules.rs` | Multi-file import graph |
| `src/project.rs` | Entry-file compilation for both VM backends |
| `examples/` | Sample `.krx` programs |
| `benchmarks/` | Performance scripts and `.krx` benches |

Register pipeline: **Lexer → Parser → Register/SSA lowering → Register VM**.

Stack pipeline: **Lexer → Parser → Compiler → Bytecode → IR/bytecode optimization → Stack VM**.

New syntax usually touches the lexer, parser, and compiler; runtime behavior
lives in `vm.rs`, while register lowering lives in `register.rs`.

## Making changes

### Language / VM changes

- Keep diffs focused; one feature or fix per PR when possible.
- Add or update an example under `examples/` when behavior is user-visible.
- Run `cargo test`, `cargo test --all-targets`, and at least `test.krx` plus
  any related example before submitting.
- Test language/runtime changes with both backends when appropriate:
  `KRIA_BACKEND=register` (default) and `KRIA_BACKEND=stack`.
- If you change performance-sensitive paths, consider running `./benchmarks/benchmarks.sh` (or `.\benchmarks\benchmarks.ps1` on Windows) and noting results in the PR.

### Documentation

- Update **README.md** for user-facing behavior.
- Keep README concise; long tutorials belong in `examples/` or future docs.

### Code style

- Match existing Rust style in the repo (naming, module structure, error messages as `String` with clear text).
- Avoid drive-by refactors unrelated to your change.
- Prefer readable code over clever abstractions; comments only where intent is non-obvious.

## Pull requests

1. Create a branch from `main` (or the default branch) with a descriptive name, e.g. `feat/object-keys` or `fix/repl-import-error`.
2. Ensure the project builds and tests pass.
3. Describe **what** changed and **why** in the PR description.
4. Link any related issue if one exists.

Reviewers may ask for tests, examples, or README updates before merge.

## Reporting bugs

Open an issue with:

- Kria version / commit (or `cargo build` output)
- OS and architecture
- Minimal `.krx` snippet or steps to reproduce
- Expected vs actual behavior

## Feature requests

Kria is actively evolving (modules today, package manager later). For larger features, open an issue first to discuss design — especially syntax and backward compatibility.

## Code of conduct

Be respectful and constructive in issues, reviews, and discussions. We want contributions to be welcoming and technical.

## License

By contributing, you agree that your contributions will be licensed under the same [MIT License](LICENSE) as the project.
