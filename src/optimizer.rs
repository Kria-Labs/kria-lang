/// Peephole optimizations on emitted bytecode.
/// Keep bytecode unchanged until optimizations can safely relocate jump targets.
pub fn optimize(bytecode: crate::bytecode::Bytecode) -> crate::bytecode::Bytecode {
    bytecode
}
