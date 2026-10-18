use std::env;
use std::path::Path;
use std::process;
use std::time::Instant;

use kria::project::compile_entry_file;
use kria::vm::VM;

fn main() {
    let Some(filename) = env::args().nth(1) else {
        eprintln!("Usage: kria_bench <file.krx>");
        process::exit(2);
    };

    let compile_started = Instant::now();
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

    eprintln!("BENCH_COMPILE_NS={compile_ns}");
    eprintln!("BENCH_EXECUTION_NS={execution_ns}");
    if let Err(error) = result {
        eprintln!("{error}");
        process::exit(1);
    }
}
