# Contributing to Kria

Thank you for your interest in Kria. This document explains how to get set up, make changes, and open pull requests.

## Getting started

1. **Fork** the repository and clone your fork.
2. Install a recent **stable Rust** toolchain ([rustup](https://rustup.rs/)).
3. Build and run tests:

```bash
cargo build
cargo test
cargo build --release
./target/release/kria test.krx
```

On Windows, run the built executable as `.\target\release\kria.exe test.krx`.

4. Explore [`examples/`](examples/) to see language features in context.

## Project layout

| Path | Purpose |
|------|---------|
| `src/lexer.rs` | Tokenization |
| `src/parser.rs` | AST / syntax |
| `src/compiler.rs` | Bytecode generation and compilation pipeline |
| `src/ir/` | IR lifting, optimization, and bytecode emission |
| `src/optimizer.rs` | Bytecode peephole optimization |
| `src/vm.rs` | Runtime |
| `src/repl.rs` | Interactive REPL |
| `src/modules.rs` | Multi-file import graph |
| `src/project.rs` | Entry-file compile pipeline |
| `examples/` | Sample `.krx` programs |
| `benchmarks/` | Performance scripts and `.krx` benches |

Pipeline: **Lexer → Parser → Compiler → Bytecode → IR optimization → Bytecode → VM**. New syntax usually touches lexer, parser, and compiler; runtime behavior lives in `vm.rs`.

## Making changes

### Language / VM changes

- Keep diffs focused; one feature or fix per PR when possible.
- Add or update an example under `examples/` when behavior is user-visible.
- Run `cargo test` and at least `test.krx` plus any related example before submitting.
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
