use std::collections::HashMap;

use super::super::inst::IrInst;
use super::super::types::IrModule;
use super::super::value::ValueId;

pub fn run(module: &mut IrModule) {
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut local_constants: HashMap<u32, u32> = HashMap::new();
            let mut value_constants: HashMap<ValueId, u32> = HashMap::new();

            for inst in &mut block.insts {
                match inst {
                    IrInst::Const { result, idx } | IrInst::FoldedBinOp { result, idx, .. } => {
                        value_constants.insert(*result, *idx);
                    }
                    IrInst::StoreLocal { slot, value } => {
                        if let Some(&constant) = value_constants.get(value) {
                            local_constants.insert(*slot, constant);
                        } else {
                            local_constants.remove(slot);
                        }
                    }
                    IrInst::LoadLocal { result, slot } => {
                        if let Some(&idx) = local_constants.get(slot) {
                            let result = *result;
                            value_constants.insert(result, idx);
                            *inst = IrInst::Const { result, idx };
                        }
                    }
                    IrInst::IncLocal { slot }
                    | IrInst::AddLocal { slot, .. }
                    | IrInst::LoopIncLessLocal { slot, .. }
                    | IrInst::LoopStepLessLocal { slot, .. } => {
                        local_constants.remove(slot);
                    }
                    IrInst::Jump { .. }
                    | IrInst::JumpIfFalse { .. }
                    | IrInst::LessConstJumpIfFalse { .. }
                    | IrInst::ForInArrayHeader { .. }
                    | IrInst::ForInArrayNext { .. }
                    | IrInst::Return { .. } => {
                        local_constants.clear();
                    }
                    _ => {
                        if let Some(result) = inst.result() {
                            value_constants.remove(&result);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_constant_is_propagated_to_later_loads() {
        let mut module = IrModule::new();
        module.functions[0].blocks[0].insts = vec![
            IrInst::Const {
                result: ValueId(0),
                idx: 0,
            },
            IrInst::StoreLocal {
                slot: 0,
                value: ValueId(0),
            },
            IrInst::LoadLocal {
                result: ValueId(1),
                slot: 0,
            },
        ];

        run(&mut module);
        assert!(matches!(
            module.functions[0].blocks[0].insts[2],
            IrInst::Const {
                result: ValueId(1),
                idx: 0
            }
        ));
    }

    #[test]
    fn local_constant_does_not_propagate_across_control_flow() {
        let mut module = IrModule::new();
        module.functions[0].blocks[0].insts = vec![
            IrInst::Const {
                result: ValueId(0),
                idx: 0,
            },
            IrInst::StoreLocal {
                slot: 0,
                value: ValueId(0),
            },
            IrInst::Jump { target_ip: 30 },
            IrInst::LoadLocal {
                result: ValueId(1),
                slot: 0,
            },
        ];

        run(&mut module);
        assert!(matches!(
            module.functions[0].blocks[0].insts[3],
            IrInst::LoadLocal {
                result: ValueId(1),
                slot: 0
            }
        ));
    }
}
