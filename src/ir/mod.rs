pub mod builder;
pub mod emit;
pub mod inst;
pub mod lift;
pub mod lower;
pub mod opt;
pub mod recorder;
pub mod ssa;
pub mod types;
pub mod value;

#[cfg(test)]
mod tests;

use crate::lexer::Lexer;
use crate::parser::Parser;

pub use emit::emit_module;
pub use lift::lift_bytecode;
pub use opt::run_ir_passes;
pub use recorder::IrRecorder;
pub use types::IrModule;

/// Compile source to IR via bytecode lift (AST → bytecode → IR).
pub fn compile_to_ir(source: &str) -> Result<IrModule, String> {
    let tokens = Lexer::new(source).tokenize();
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse()?;
    let mut compiler = crate::compiler::Compiler::new();
    compiler.compile_module(&stmts)?;
    let mut ir = lift_bytecode(compiler.bytecode());
    run_ir_passes(&mut ir);
    Ok(ir)
}

pub fn dump_ir(module: &IrModule) -> String {
    if std::env::var("KRIA_DUMP_IR").ok().as_deref() != Some("1") {
        return String::new();
    }
    let mut out = String::new();
    for (fi, func) in module.functions.iter().enumerate() {
        out.push_str(&format!("function {} ", fi));
        if let Some(n) = &func.name {
            out.push_str(n);
        }
        out.push('\n');
        for block in &func.blocks {
            out.push_str(&format!("  block{}:\n", block.id.0));
            for phi in &block.phis {
                out.push_str(&format!(
                    "    phi %{} ({}) <- {:?}\n",
                    phi.result.0, phi.var_name, phi.incoming
                ));
            }
            for inst in &block.insts {
                out.push_str(&format!("    {:?}\n", inst));
            }
            if let Some(t) = &block.term {
                out.push_str(&format!("    term {:?}\n", t));
            }
        }
    }
    out
}
