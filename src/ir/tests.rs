#[cfg(test)]
mod tests {
    use super::super::inst::{IrBinOp, IrInst};
    use super::super::types::IrModule;
    use super::super::value::ValueId;
    use super::super::{emit_module, run_ir_passes};

    #[test]
    fn ir_module_roundtrip_const_add() {
        let mut module = IrModule::new();
        module.constants = vec![
            crate::vm::Value::Number(1),
            crate::vm::Value::Number(2),
        ];
        let entry = module.entry_function_mut();
        let block = entry.entry;
        entry.block_mut(block).insts = vec![
            IrInst::Const {
                result: ValueId(0),
                idx: 0,
            },
            IrInst::Const {
                result: ValueId(1),
                idx: 1,
            },
            IrInst::BinOp {
                result: ValueId(2),
                op: IrBinOp::Add,
                left: ValueId(0),
                right: ValueId(1),
            },
            IrInst::Print {
                value: ValueId(2),
            },
        ];
        run_ir_passes(&mut module);
        let bc = emit_module(&module);
        assert!(bc.code.contains(&crate::bytecode::OP_CONSTANT));
        assert!(bc.code.contains(&crate::bytecode::OP_PRINT));
    }

    #[test]
    fn ir_pipeline_matches_direct_compile() {
        use crate::compiler::Compiler;
        use crate::lexer::Lexer;
        use crate::optimizer::optimize;
        use crate::parser::Parser;
        use crate::vm::VM;

        let src = "set x = 1 + 2\nprint(x)\nfn add(a, b) { return a + b }\nprint(add(3, 4))";
        let tokens = Lexer::new(src).tokenize();
        let stmts = Parser::new(tokens).parse().expect("parse");

        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let via_lift = optimize(crate::ir::emit_module(&crate::ir::lift_bytecode(
            compiler.bytecode(),
        )));

        let mut compiler2 = Compiler::new();
        compiler2.compile_module(&stmts).expect("compile");
        let via_finish = compiler2.finish_bytecode();

        let mut vm1 = VM::new();
        let mut vm2 = VM::new();
        vm1.execute(&via_lift).expect("lift+emit");
        vm2.execute(&via_finish).expect("finish_bytecode");
    }
}
