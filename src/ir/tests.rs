#[cfg(test)]
mod tests {
    use super::super::inst::{IrBinOp, IrInst};
    use super::super::types::IrModule;
    use super::super::value::ValueId;
    use super::super::{emit_module, run_ir_passes};

    #[test]
    fn ir_module_roundtrip_const_add() {
        let mut module = IrModule::new();
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
        assert!(bc.code.contains(&crate::bytecode::OP_ADD));
        assert!(bc.code.contains(&crate::bytecode::OP_PRINT));
    }
}
