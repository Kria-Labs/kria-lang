use crate::bytecode::*;
use crate::vm::Value;

/// Peephole optimizations on emitted bytecode.
pub fn optimize(mut bytecode: Bytecode) -> Bytecode {
    peephole(&mut bytecode);
    bytecode
}

fn peephole(bytecode: &mut Bytecode) {
    let mut i = 0;
    while i < bytecode.code.len() {
        let op = bytecode.code[i];
        // CONSTANT idx; POP  ->  (remove dead literal)
        if op == OP_CONSTANT && i + 5 < bytecode.code.len() && bytecode.code[i + 5] == OP_POP {
            let const_idx = Bytecode::read_u32(&bytecode.code, i + 1) as usize;
            if const_idx < bytecode.constants.len() {
                let c = &bytecode.constants[const_idx];
                let safe = matches!(c, Value::Number(_) | Value::Boolean(_) | Value::Null | Value::String(_));
                if safe {
                    bytecode.code.drain(i..i + 6);
                    continue;
                }
            }
        }
        i += 1;
    }
}
