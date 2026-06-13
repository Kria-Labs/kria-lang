use std::collections::HashMap;

use super::inst::{IrBlock, IrInst, IrPhi, IrTerminator};
use super::types::{IrFunction, IrModule};
use super::value::{BlockId, FunctionId, ValueId};

#[derive(Debug, Clone)]
struct SsaVar {
    current: ValueId,
    incomplete: HashMap<BlockId, ValueId>,
}

#[derive(Debug, Default)]
struct BlockSsa {
    sealed: bool,
    incomplete_phis: HashMap<String, ValueId>,
}

pub struct IrBuilder {
    pub func_id: FunctionId,
    pub current_block: BlockId,
    next_value: u32,
    ssa_vars: HashMap<String, SsaVar>,
    block_ssa: HashMap<BlockId, BlockSsa>,
    pub loop_stack: Vec<LoopContext>,
}

#[derive(Debug, Clone)]
pub struct LoopContext {
    pub header: BlockId,
    pub exit: BlockId,
    pub continue_target: BlockId,
    pub break_targets: Vec<BlockId>,
    pub continue_targets: Vec<BlockId>,
}

impl IrBuilder {
    pub fn new(_module: &IrModule, func_id: FunctionId) -> Self {
        IrBuilder {
            func_id,
            current_block: BlockId(0),
            next_value: 0,
            ssa_vars: HashMap::new(),
            block_ssa: HashMap::new(),
            loop_stack: Vec::new(),
        }
    }

    pub fn function<'a>(&self, module: &'a IrModule) -> &'a IrFunction {
        &module.functions[self.func_id.0 as usize]
    }

    pub fn function_mut<'a>(&self, module: &'a mut IrModule) -> &'a mut IrFunction {
        &mut module.functions[self.func_id.0 as usize]
    }

    pub fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    pub fn emit_inst(&mut self, module: &mut IrModule, inst: IrInst) {
        let block = self.current_block;
        self.function_mut(module).block_mut(block).insts.push(inst);
    }

    pub fn set_terminator(&mut self, module: &mut IrModule, term: IrTerminator) {
        let block = self.current_block;
        self.function_mut(module).block_mut(block).term = Some(term);
    }

    pub fn new_block(&mut self, module: &mut IrModule) -> BlockId {
        self.function_mut(module).new_block()
    }

    pub fn switch_block(&mut self, block: BlockId) {
        self.current_block = block;
    }

    pub fn add_pred(&mut self, module: &mut IrModule, block: BlockId, pred: BlockId) {
        let b = self.function_mut(module).block_mut(block);
        if !b.preds.contains(&pred) {
            b.preds.push(pred);
        }
    }

    pub fn init_param_ssa(&mut self, module: &mut IrModule, name: &str, local_index: u32) {
        let result = self.fresh_value();
        self.emit_inst(
            module,
            IrInst::LoadLocal {
                result,
                slot: local_index,
            },
        );
        self.ssa_vars.insert(
            name.to_string(),
            SsaVar {
                current: result,
                incomplete: HashMap::new(),
            },
        );
    }

    pub fn read_local_ssa(&mut self, module: &mut IrModule, name: &str) -> ValueId {
        let block = self.current_block;
        if !self.is_block_sealed(module, block) {
            if self.ssa_vars.contains_key(name) {
                let phi_result = self.fresh_value();
                self.function_mut(module)
                    .block_mut(block)
                    .phis
                    .push(IrPhi {
                        result: phi_result,
                        var_name: name.to_string(),
                        incoming: Vec::new(),
                    });
                self.block_ssa
                    .entry(block)
                    .or_default()
                    .incomplete_phis
                    .insert(name.to_string(), phi_result);
                return phi_result;
            }
        }
        self.ssa_vars
            .get(name)
            .map(|v| v.current)
            .unwrap_or(ValueId::INVALID)
    }

    pub fn write_local_ssa(&mut self, name: &str, value: ValueId) {
        let block = self.current_block;
        if self.block_ssa.get(&block).map(|b| b.sealed).unwrap_or(false) {
            if let Some(var) = self.ssa_vars.get_mut(name) {
                var.current = value;
            } else {
                self.ssa_vars.insert(
                    name.to_string(),
                    SsaVar {
                        current: value,
                        incomplete: HashMap::new(),
                    },
                );
            }
        } else if let Some(var) = self.ssa_vars.get_mut(name) {
            var.incomplete.insert(block, value);
        } else {
            self.ssa_vars.insert(
                name.to_string(),
                SsaVar {
                    current: value,
                    incomplete: HashMap::new(),
                },
            );
        }
    }

    fn is_block_sealed(&self, module: &IrModule, block: BlockId) -> bool {
        module.functions[self.func_id.0 as usize]
            .block(block)
            .sealed
    }

    pub fn seal_block(&mut self, module: &mut IrModule, block: BlockId) {
        let preds: Vec<BlockId> = module.functions[self.func_id.0 as usize].block(block).preds.clone();
        let incomplete: HashMap<String, ValueId> = self
            .block_ssa
            .get(&block)
            .map(|b| b.incomplete_phis.clone())
            .unwrap_or_default();

        for (name, phi_result) in incomplete {
            let mut incoming = Vec::new();
            for pred in &preds {
                let val = self
                    .ssa_vars
                    .get(&name)
                    .and_then(|v| v.incomplete.get(pred).copied())
                    .or_else(|| self.ssa_vars.get(&name).map(|v| v.current))
                    .unwrap_or(ValueId::INVALID);
                incoming.push((val, *pred));
            }
            let func = &mut module.functions[self.func_id.0 as usize];
            if let Some(phi) = func
                .block_mut(block)
                .phis
                .iter_mut()
                .find(|p| p.result == phi_result)
            {
                phi.incoming = incoming.clone();
            }
            if let Some(last) = incoming.last() {
                if last.0 != ValueId::INVALID {
                    if let Some(var) = self.ssa_vars.get_mut(&name) {
                        var.current = last.0;
                    }
                }
            }
        }

        module.functions[self.func_id.0 as usize].block_mut(block).sealed = true;
        if let Some(bs) = self.block_ssa.get_mut(&block) {
            bs.sealed = true;
            bs.incomplete_phis.clear();
        }
    }

    pub fn branch(
        &mut self,
        module: &mut IrModule,
        cond: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    ) {
        let from = self.current_block;
        self.set_terminator(
            module,
            IrTerminator::Branch {
                cond,
                then_block,
                else_block,
            },
        );
        self.add_pred(module, then_block, from);
        self.add_pred(module, else_block, from);
    }

    pub fn jump(&mut self, module: &mut IrModule, target: BlockId) {
        let from = self.current_block;
        self.set_terminator(module, IrTerminator::Jump { target });
        self.add_pred(module, target, from);
    }

    pub fn return_val(&mut self, module: &mut IrModule, value: Option<ValueId>) {
        self.set_terminator(module, IrTerminator::Return { value });
    }
}
