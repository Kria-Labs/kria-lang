use std::collections::HashMap;

use super::super::inst::{IrInst, IrTerminator};
use super::super::types::IrModule;
use super::super::value::ValueId;

/// Mark unused side-effect-free values so the bytecode pass can remove their stack pairs.
pub fn run(module: &mut IrModule) {
    for function in &mut module.functions {
        let mut uses: HashMap<ValueId, usize> = HashMap::new();
        for block in &function.blocks {
            for phi in &block.phis {
                for (value, _) in &phi.incoming {
                    *uses.entry(*value).or_default() += 1;
                }
            }
            for inst in &block.insts {
                inst.visit_uses(|value| *uses.entry(value).or_default() += 1);
            }
            if let Some(terminator) = &block.term {
                match terminator {
                    IrTerminator::Branch { cond, .. } => {
                        *uses.entry(*cond).or_default() += 1;
                    }
                    IrTerminator::Return { value: Some(value) } => {
                        *uses.entry(*value).or_default() += 1;
                    }
                    IrTerminator::Jump { .. }
                    | IrTerminator::Return { value: None }
                    | IrTerminator::ForInArray { .. } => {}
                }
            }
        }

        for block in &mut function.blocks {
            let mut out = Vec::with_capacity(block.insts.len());
            for inst in block.insts.drain(..) {
                let is_discardable = matches!(
                    &inst,
                    IrInst::Const { .. }
                        | IrInst::Null { .. }
                        | IrInst::LoadGlobal { .. }
                        | IrInst::LoadLocal { .. }
                        | IrInst::LoadUpvalue { .. }
                );
                if is_discardable
                    && inst
                        .result()
                        .map_or(false, |result| uses.get(&result).copied().unwrap_or(0) == 0)
                {
                    out.push(IrInst::Discard {
                        instruction: Box::new(inst),
                    });
                } else {
                    out.push(inst);
                }
            }
            block.insts = out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_condition_keeps_its_stack_producer_live() {
        let mut module = IrModule::new();
        module.functions[0].blocks[0].insts = vec![
            IrInst::Const {
                result: ValueId(0),
                idx: 0,
            },
            IrInst::JumpIfFalse {
                cond: ValueId(0),
                target_ip: 20,
            },
        ];

        run(&mut module);
        assert!(matches!(
            module.functions[0].blocks[0].insts[0],
            IrInst::Const { .. }
        ));
    }

    #[test]
    fn unused_constant_is_marked_for_stack_balanced_discard() {
        let mut module = IrModule::new();
        module.functions[0].blocks[0].insts = vec![IrInst::Const {
            result: ValueId(0),
            idx: 0,
        }];

        run(&mut module);
        assert!(matches!(
            module.functions[0].blocks[0].insts[0],
            IrInst::Discard { .. }
        ));
    }
}
