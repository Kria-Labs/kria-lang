use std::collections::{HashMap, HashSet};

use crate::bytecode::*;

use super::inst::{IrBinOp, IrInst, IrTerminator};
use super::types::{IrFunction, IrModule};
use super::value::{BlockId, FunctionId};

pub struct IrEmitter {
    bytecode: Bytecode,
    emitted_functions: HashSet<FunctionId>,
    function_offsets: HashMap<FunctionId, u32>,
}

impl IrEmitter {
    fn new(module: &IrModule) -> Self {
        IrEmitter {
            bytecode: Bytecode {
                code: Vec::new(),
                constants: module.constants.clone(),
                num_globals: module.num_globals(),
            },
            emitted_functions: HashSet::new(),
            function_offsets: HashMap::new(),
        }
    }

    fn emit_inst(&mut self, inst: &IrInst, module: &IrModule) {
        match inst {
            IrInst::Const { idx, .. } => {
                self.bytecode.emit_byte(OP_CONSTANT);
                self.bytecode.emit_u32(*idx);
            }
            IrInst::Null { .. } => {
                self.bytecode.emit_byte(OP_NULL);
            }
            IrInst::LoadGlobal { slot, .. } => {
                self.bytecode.emit_byte(OP_LOAD_GLOBAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::StoreGlobal { slot, .. } => {
                self.bytecode.emit_byte(OP_STORE_GLOBAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::IncGlobal { slot } => {
                self.bytecode.emit_byte(OP_INC_GLOBAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::AddGlobal { slot, delta } => {
                self.bytecode.emit_byte(OP_ADD_GLOBAL);
                self.bytecode.emit_u32(*slot);
                self.bytecode.emit_i64(*delta);
            }
            IrInst::LoadLocal { slot, .. } => {
                self.bytecode.emit_byte(OP_LOAD_LOCAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::StoreLocal { slot, .. } => {
                self.bytecode.emit_byte(OP_STORE_LOCAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::LoadUpvalue { slot, .. } => {
                self.bytecode.emit_byte(OP_LOAD_UPVALUE);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::StoreUpvalue { slot, .. } => {
                self.bytecode.emit_byte(OP_STORE_UPVALUE);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::BinOp { op, .. } => {
                let opcode = match op {
                    IrBinOp::Add => OP_ADD,
                    IrBinOp::Sub => OP_SUBTRACT,
                    IrBinOp::Mul => OP_MULTIPLY,
                    IrBinOp::Div => OP_DIVIDE,
                    IrBinOp::Eq => OP_EQUALS,
                    IrBinOp::Ne => OP_NOT_EQUALS,
                    IrBinOp::Gt => OP_GREATER,
                    IrBinOp::Lt => OP_LESS,
                    IrBinOp::Ge => OP_GREATER_EQUAL,
                    IrBinOp::Le => OP_LESS_EQUAL,
                };
                self.bytecode.emit_byte(opcode);
            }
            IrInst::Not { .. } => {
                self.bytecode.emit_byte(OP_NOT);
            }
            IrInst::Print { .. } => {
                self.bytecode.emit_byte(OP_PRINT);
            }
            IrInst::Pop { .. } => {
                self.bytecode.emit_byte(OP_POP);
            }
            IrInst::Input { type_mask, .. } => {
                self.bytecode.emit_byte(OP_INPUT);
                self.bytecode.emit_byte(*type_mask);
            }
            IrInst::Call { args, .. } => {
                self.bytecode.emit_byte(OP_CALL_FUNCTION);
                self.bytecode.emit_u32(args.len() as u32);
            }
            IrInst::MakeClosure { func, .. } => self.emit_make_closure(*func, module),
            IrInst::BuildArray { elements, mutable, .. } => {
                self.bytecode.emit_byte(OP_BUILD_ARRAY);
                self.bytecode.emit_u32(elements.len() as u32);
                self.bytecode.emit_byte(if *mutable { 1 } else { 0 });
            }
            IrInst::BuildObject { field_values, field_keys, .. } => {
                self.bytecode.emit_byte(OP_BUILD_OBJECT);
                self.bytecode.emit_u32(field_values.len() as u32);
                for k in field_keys {
                    self.bytecode.emit_u32(*k);
                }
            }
            IrInst::IndexGet { .. } => {
                self.bytecode.emit_byte(OP_INDEX_GET);
            }
            IrInst::IndexSet { .. } => {
                self.bytecode.emit_byte(OP_INDEX_SET);
            }
            IrInst::ArrayLen { .. } => {
                self.bytecode.emit_byte(OP_ARRAY_LEN);
            }
            IrInst::ArrayPush { .. } => {
                self.bytecode.emit_byte(OP_ARRAY_PUSH);
                self.bytecode.emit_byte(OP_NULL);
            }
            IrInst::ArrayPop { .. } => {
                self.bytecode.emit_byte(OP_ARRAY_POP);
            }
            IrInst::ObjectGetConst { key_idx, .. } => {
                self.bytecode.emit_byte(OP_OBJECT_GET_CONST);
                self.bytecode.emit_u32(*key_idx);
            }
            IrInst::ObjectGet { .. } => {
                self.bytecode.emit_byte(OP_OBJECT_GET);
            }
            IrInst::ObjectSet { .. } => {
                self.bytecode.emit_byte(OP_OBJECT_SET);
            }
            IrInst::ObjectDelete { .. } => {
                self.bytecode.emit_byte(OP_OBJECT_DELETE);
            }
            IrInst::ObjectKeys { .. } => {
                self.bytecode.emit_byte(OP_OBJECT_KEYS);
            }
            IrInst::ObjectGetOrCreateConst { key_idx, .. } => {
                self.bytecode.emit_byte(OP_OBJECT_GET_OR_CREATE_CONST);
                self.bytecode.emit_u32(*key_idx);
            }
            IrInst::ObjectGetOrCreate { .. } => {
                self.bytecode.emit_byte(OP_OBJECT_GET_OR_CREATE);
            }
            IrInst::IsArray { .. } => {
                self.bytecode.emit_byte(OP_IS_ARRAY);
            }
            IrInst::IsObject { .. } => {
                self.bytecode.emit_byte(OP_IS_OBJECT);
            }
            IrInst::MemberLength { .. } => {
                self.bytecode.emit_byte(OP_MEMBER_LENGTH);
            }
            IrInst::Type { .. } => {
                self.bytecode.emit_byte(OP_TYPE);
            }
            IrInst::Wait { .. } => {
                self.bytecode.emit_byte(OP_WAIT);
                self.bytecode.emit_byte(OP_NULL);
            }
            IrInst::LoopIncLess { global_idx, limit } => {
                self.bytecode.emit_byte(OP_LOOP_INC_LESS);
                self.bytecode.emit_u32(*global_idx);
                self.bytecode.emit_i64(*limit);
            }
            IrInst::LoopStepLess {
                global_idx,
                limit,
                step,
            } => {
                self.bytecode.emit_byte(OP_LOOP_STEP_LESS);
                self.bytecode.emit_u32(*global_idx);
                self.bytecode.emit_i64(*limit);
                self.bytecode.emit_i64(*step);
            }
            IrInst::StorePathCur { slot, .. } => {
                self.bytecode.emit_byte(OP_STORE_GLOBAL);
                self.bytecode.emit_u32(*slot);
            }
            IrInst::LoadPathCur { slot, .. } => {
                self.bytecode.emit_byte(OP_LOAD_GLOBAL);
                self.bytecode.emit_u32(*slot);
            }
        }
    }

    fn emit_make_closure(&mut self, func_id: FunctionId, module: &IrModule) {
        if !self.emitted_functions.contains(&func_id) {
            self.bytecode.emit_byte(OP_JUMP);
            let skip = self.bytecode.emit_u32(0);
            let offset = self.bytecode.code.len() as u32;
            self.function_offsets.insert(func_id, offset);
            self.emit_function_with_labels(&module.functions[func_id.0 as usize], module);
            self.bytecode.patch_u32(skip, self.bytecode.code.len() as u32);
            self.emitted_functions.insert(func_id);
        }
        let func = &module.functions[func_id.0 as usize];
        self.bytecode.emit_byte(OP_MAKE_CLOSURE);
        self.bytecode.emit_u32(self.function_offsets[&func_id]);
        self.bytecode.emit_u32(func.params.len() as u32);
        self.bytecode.emit_u32(func.captures.len() as u32);
        for cap in &func.captures {
            self.bytecode.emit_byte(cap.kind);
            self.bytecode.emit_u32(cap.index);
        }
    }

    fn emit_function_with_labels(&mut self, func: &IrFunction, module: &IrModule) {
        let mut labels: HashMap<BlockId, u32> = HashMap::new();
        let mut pending: Vec<(usize, BlockId)> = Vec::new();
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        schedule_blocks(func.entry, func, &mut order, &mut seen);

        for block_id in &order {
            labels.insert(*block_id, self.bytecode.code.len() as u32);
            let block = &func.blocks[block_id.0 as usize];
            for inst in &block.insts {
                self.emit_inst(inst, module);
            }
            match &block.term {
                Some(IrTerminator::Jump { target }) => {
                    self.bytecode.emit_byte(OP_JUMP);
                    pending.push((self.bytecode.emit_u32(0), *target));
                }
                Some(IrTerminator::Branch { else_block, .. }) => {
                    self.bytecode.emit_byte(OP_JUMP_IF_FALSE);
                    pending.push((self.bytecode.emit_u32(0), *else_block));
                }
                Some(IrTerminator::Return { value }) => {
                    if value.is_none() {
                        self.bytecode.emit_byte(OP_NULL);
                    }
                    self.bytecode.emit_byte(OP_RETURN);
                }
                Some(IrTerminator::ForInArray {
                    arr_slot,
                    i_slot,
                    item_local,
                    exit_block,
                    ..
                }) => {
                    let header = self.bytecode.code.len();
                    self.bytecode.emit_byte(OP_FOR_IN_ARRAY_HEADER);
                    self.bytecode.emit_u32(*arr_slot);
                    self.bytecode.emit_u32(*i_slot);
                    pending.push((self.bytecode.emit_u32(0), *exit_block));
                    self.bytecode.emit_byte(OP_STORE_LOCAL);
                    self.bytecode.emit_u32(*item_local);
                    let _ = header;
                }
                None => {}
            }
        }

        for (pos, target) in pending {
            if let Some(&addr) = labels.get(&target) {
                self.bytecode.patch_u32(pos, addr);
            }
        }
    }
}

pub fn emit_module(module: &IrModule) -> Bytecode {
    let mut emitter = IrEmitter::new(module);
    emitter.emit_function_with_labels(module.entry_function(), module);
    emitter.bytecode.num_globals = module.num_globals();
    emitter.bytecode
}

fn schedule_blocks(
    id: BlockId,
    func: &IrFunction,
    order: &mut Vec<BlockId>,
    seen: &mut HashSet<BlockId>,
) {
    if !seen.insert(id) {
        return;
    }
    order.push(id);
    let block = &func.blocks[id.0 as usize];
    if let Some(term) = &block.term {
        match term {
            IrTerminator::Jump { target } => schedule_blocks(*target, func, order, seen),
            IrTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                schedule_blocks(*then_block, func, order, seen);
                schedule_blocks(*else_block, func, order, seen);
            }
            IrTerminator::ForInArray { body, exit_block, .. } => {
                schedule_blocks(*body, func, order, seen);
                schedule_blocks(*exit_block, func, order, seen);
            }
            IrTerminator::Return { .. } => {}
        }
    }
    for block in &func.blocks {
        if let Some(IrTerminator::Jump { target }) = &block.term {
            schedule_blocks(*target, func, order, seen);
        }
    }
}
