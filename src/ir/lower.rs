//! AST → IR lowering is performed via bytecode lift (`lift_bytecode`).
//! Direct AST lowering may be added here in the future.
use super::types::IrModule;

pub struct AstLowerer;

impl AstLowerer {
    pub fn lower_module(
        _module: &mut IrModule,
        _statements: &[crate::ast::Statement],
    ) -> Result<std::collections::HashMap<String, usize>, String> {
        Err("Use compile_to_ir() or Compiler::finish_bytecode() pipeline".to_string())
    }
}
