use std::collections::{HashMap, HashSet};

use crate::bytecode::*;

use super::inst::{IrBinOp, IrInst};
use super::types::{IrFunction, IrModule};
use super::value::FunctionId;

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
            IrInst::Jump { .. }
            | IrInst::JumpIfFalse { .. }
            | IrInst::Return { .. }
            | IrInst::LessConstJumpIfFalse { .. }
            | IrInst::ForInArrayHeader { .. }
            | IrInst::ForInArrayNext { .. } => {
                unreachable!("control flow handled in emit_function_linear")
            }
        }
    }

    fn emit_make_closure(&mut self, func_id: FunctionId, module: &IrModule) {
        if !self.emitted_functions.contains(&func_id) {
            self.bytecode.emit_byte(OP_JUMP);
            let skip = self.bytecode.emit_u32(0);
            let offset = self.bytecode.code.len() as u32;
            self.function_offsets.insert(func_id, offset);
            self.emit_function_linear(&module.functions[func_id.0 as usize], module);
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

    fn emit_function_linear(&mut self, func: &IrFunction, module: &IrModule) {
        let block = &func.blocks[func.entry.0 as usize];
        let base_emit = self.bytecode.code.len() as u32;
        let base_lift = func.lift_start;
        let mut fixups: Vec<(usize, u32)> = Vec::new();
        let mut ip_map: HashMap<u32, u32> = HashMap::new();

        for (i, inst) in block.insts.iter().enumerate() {
            let old_ip = block.inst_ips.get(i).copied().unwrap_or_else(|| {
                if i == 0 {
                    func.lift_start
                } else {
                    block.inst_ips.get(i - 1).copied().unwrap_or(func.lift_start)
                        + block.insts[i - 1].lifted_byte_size_with(module) as u32
                }
            });
            ip_map.insert(old_ip, self.bytecode.code.len() as u32);

            match inst {
                IrInst::Jump { target_ip } => {
                    self.bytecode.emit_byte(OP_JUMP);
                    fixups.push((self.bytecode.emit_u32(0), *target_ip));
                }
                IrInst::JumpIfFalse { target_ip } => {
                    self.bytecode.emit_byte(OP_JUMP_IF_FALSE);
                    fixups.push((self.bytecode.emit_u32(0), *target_ip));
                }
                IrInst::Return { value } => {
                    if value.is_none() {
                        self.bytecode.emit_byte(OP_NULL);
                    }
                    self.bytecode.emit_byte(OP_RETURN);
                }
                IrInst::LessConstJumpIfFalse {
                    global_idx,
                    limit,
                    target_ip,
                } => {
                    self.bytecode.emit_byte(OP_LESS_CONST_JUMP_IF_FALSE);
                    self.bytecode.emit_u32(*global_idx);
                    self.bytecode.emit_i64(*limit);
                    fixups.push((self.bytecode.emit_u32(0), *target_ip));
                }
                IrInst::ForInArrayHeader {
                    arr_slot,
                    i_slot,
                    exit_ip,
                } => {
                    self.bytecode.emit_byte(OP_FOR_IN_ARRAY_HEADER);
                    self.bytecode.emit_u32(*arr_slot);
                    self.bytecode.emit_u32(*i_slot);
                    fixups.push((self.bytecode.emit_u32(0), *exit_ip));
                }
                IrInst::ForInArrayNext {
                    i_slot,
                    loop_start_ip,
                } => {
                    self.bytecode.emit_byte(OP_FOR_IN_ARRAY_NEXT);
                    self.bytecode.emit_u32(*i_slot);
                    fixups.push((self.bytecode.emit_u32(0), *loop_start_ip));
                }
                IrInst::MakeClosure { func: fid, .. } => self.emit_make_closure(*fid, module),
                _ => self.emit_inst(inst, module),
            }
        }

        if let (Some(&last_ip), Some(last_inst)) = (block.inst_ips.last(), block.insts.last()) {
            let end_old = last_ip + last_inst.lifted_byte_size_with(module) as u32;
            ip_map.insert(end_old, self.bytecode.code.len() as u32);
        }

        for (pos, old_target) in fixups {
            let new_target = ip_map.get(&old_target).copied().unwrap_or_else(|| {
                base_emit + old_target.saturating_sub(base_lift)
            });
            self.bytecode.patch_u32(pos, new_target);
        }
    }
}

pub fn emit_module(module: &IrModule) -> Bytecode {
    let mut emitter = IrEmitter::new(module);
    emitter.emit_function_linear(module.entry_function(), module);
    emitter.bytecode.num_globals = module.num_globals();
    emitter.bytecode
}
