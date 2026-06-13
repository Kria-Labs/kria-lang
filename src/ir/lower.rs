//! AST → IR lowering (delegates to compiler IR recorder during compilation).
use super::types::IrModule;

pub struct AstLowerer;

impl AstLowerer {
    pub fn lower_module(
        _module: &mut IrModule,
        _statements: &[crate::ast::Statement],
    ) -> Result<std::collections::HashMap<String, usize>, String> {
        Err("Use Compiler IR recorder pipeline".to_string())
    }
}
