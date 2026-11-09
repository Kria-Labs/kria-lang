use std::env;
use std::path::Path;
use std::process;
use std::time::Instant;

use kria::project::{compile_entry_file, compile_register_entry_file};
use kria::vm::{RegisterVM, VM};

fn main() {
    let Some(filename) = env::args().nth(1) else {
        eprintln!("Usage: kria_bench <file.krx>");
        process::exit(2);
    };

    let compile_started = Instant::now();
    let backend = env::var("KRIA_BACKEND")
        .unwrap_or_else(|_| "register".to_string())
        .to_lowercase();
    let (compile_ns, execution_ns, result) = match backend.as_str() {
        "stack" => {
            let bytecode = match compile_entry_file(Path::new(&filename)) {
                Ok(bytecode) => bytecode,
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            };
            let compile_ns = compile_started.elapsed().as_nanos();
            let execution_started = Instant::now();
            let result = VM::new().execute(&bytecode);
            let execution_ns = execution_started.elapsed().as_nanos();
            (compile_ns, execution_ns, result)
        }
        "jit" | "llvm" => {
            let module = match compile_register_entry_file(Path::new(&filename)) {
                Ok(module) => module,
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            };
            let jit = kria::jit::LlvmJit::new();
            let compiled = match jit.compile(&module) {
                Ok(c) => c,
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            };
            let compile_ns = compile_started.elapsed().as_nanos();
            let execution_started = Instant::now();
            let result = compiled.run();
            let execution_ns = execution_started.elapsed().as_nanos();
            (compile_ns, execution_ns, result)
        }
        _ => {
            let module = match compile_register_entry_file(Path::new(&filename)) {
                Ok(module) => module,
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            };
            let compile_ns = compile_started.elapsed().as_nanos();
            let execution_started = Instant::now();
            let result = RegisterVM::new().execute(&module);
            let execution_ns = execution_started.elapsed().as_nanos();
            (compile_ns, execution_ns, result)
        }
    };

    eprintln!("BENCH_COMPILE_NS={compile_ns}");
    eprintln!("BENCH_EXECUTION_NS={execution_ns}");
    if let Err(error) = result {
        eprintln!("{error}");
        process::exit(1);
    }
}
