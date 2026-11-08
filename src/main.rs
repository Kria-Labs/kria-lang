use std::env;
use std::process;

use kria::project::{compile_entry_file, compile_register_entry_file};
use kria::vm::RegisterVM;
use kria::vm::VM;

fn print_usage(program: &str) {
    eprintln!("Kria programming language");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {}              Start interactive REPL", program);
    eprintln!("  {} <file.krx>   Run a Kria source file", program);
    eprintln!("  {} -h, --help   Show this help", program);
    eprintln!("  {} -v, --version Show version", program);
}

fn print_version() {
    println!("Kria {}", env!("CARGO_PKG_VERSION"));
}

fn run_file(filename: &str) -> Result<(), String> {
    let path = std::path::Path::new(filename);
    if env::var("KRIA_BACKEND").unwrap_or_else(|_| "register".to_string()).to_lowercase() == "stack" {
        let bytecode = compile_entry_file(path)?;
        let mut vm = VM::new();
        vm.execute(&bytecode)?;
    } else {
        let module = compile_register_entry_file(path)?;
        let mut vm = RegisterVM::new();
        vm.execute(&module)?;
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        if let Err(e) = kria::repl::run() {
            eprintln!("REPL error: {}", e);
            process::exit(1);
        }
        return;
    }

    match args[1].as_str() {
        "-h" | "--help" => {
            print_usage(&args[0]);
        }
        "-v" | "--version" => {
            print_version();
        }
        path => {
            if let Err(e) = run_file(path) {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    }
}
