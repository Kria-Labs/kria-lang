pub mod ast;
#[cfg(test)]
mod bytecode;
#[cfg(test)]
mod compile_opt;
#[cfg(test)]
mod compiler;
#[cfg(test)]
mod ir;
pub mod jit;
pub mod lexer;
pub mod modules;
#[cfg(test)]
mod optimizer;
pub mod parser;
pub mod project;
pub mod register;
pub mod repl;
pub mod vm;
