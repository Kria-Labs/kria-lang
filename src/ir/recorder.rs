//! Records stack-bytecode operations as SSA IR during legacy-style compilation.

use std::collections::HashMap;

use crate::bytecode::*;
use crate::vm::Value;

use super::inst::{IrBinOp, IrInst, IrTerminator};
use super::types::{CaptureDesc, IrFunction, IrModule, IrParam};
use super::value::{BlockId, FunctionId, ValueId};

pub struct IrRecorder {
    pub module: IrModule,
    pub current_func: FunctionId,
    next_val: u32,
    /// Stack of SSA values for expression stack simulation.
    stack: Vec<ValueId>,
    /// Pending jump patches: (bytecode_pos_placeholder, target_block) — unused in linear mode.
    #[allow(dead_code)]
    jump_patches: Vec<(usize, BlockId)>,
    /// Function being compiled when inside compile_function.
    compiling_func: Option<FunctionId>,
    /// Maps bytecode function offset -> FunctionId (filled at emit time).
    pub func_offsets: HashMap<u32, FunctionId>,
}

impl IrRecorder {
    pub fn new() -> Self {
        IrRecorder {
            module: IrModule::new(),
            current_func: FunctionId(0),
            next_val: 0,
            stack: Vec::new(),
            jump_patches: Vec::new(),
            compiling_func: None,
            func_offsets: HashMap::new(),
        }
    }

    fn fresh(&mut self) -> ValueId {
        let v = ValueId(self.next_val);
        self.next_val += 1;
        v
    }

    fn push_inst(&mut self, inst: IrInst) -> Option<ValueId> {
        let result = inst.result();
        let func = &mut self.module.functions[self.current_func.0 as usize];
        let block = func.entry;
        func.block_mut(block).insts.push(inst);
        if let Some(r) = result {
            self.stack.push(r);
            Some(r)
        } else {
            None
        }
    }

    pub fn add_constant(&mut self, value: Value) -> u32 {
        self.module.add_constant(value)
    }

    pub fn resolve_global(&mut self, name: &str) -> u32 {
        self.module.resolve_global(name)
    }

    pub fn emit_const(&mut self, idx: u32) {
        let r = self.fresh();
        self.push_inst(IrInst::Const { result: r, idx });
    }

    pub fn emit_null(&mut self) {
        let r = self.fresh();
        self.push_inst(IrInst::Null { result: r });
    }

    pub fn emit_load_global(&mut self, slot: u32) {
        let r = self.fresh();
        self.push_inst(IrInst::LoadGlobal { result: r, slot });
    }

    pub fn emit_store_global(&mut self, slot: u32) {
        let val = self.stack.pop().expect("stack underflow store global");
        self.push_inst(IrInst::StoreGlobal { slot, value: val });
    }

    pub fn emit_inc_global(&mut self, slot: u32) {
        self.push_inst(IrInst::IncGlobal { slot });
    }

    pub fn emit_add_global(&mut self, slot: u32, delta: i64) {
        self.push_inst(IrInst::AddGlobal { slot, delta });
    }

    pub fn emit_load_local(&mut self, slot: u32) {
        let r = self.fresh();
        self.push_inst(IrInst::LoadLocal { result: r, slot });
    }

    pub fn emit_store_local(&mut self, slot: u32) {
        let val = self.stack.pop().expect("stack underflow store local");
        self.push_inst(IrInst::StoreLocal { slot, value: val });
    }

    pub fn emit_load_upvalue(&mut self, slot: u32) {
        let r = self.fresh();
        self.push_inst(IrInst::LoadUpvalue { result: r, slot });
    }

    pub fn emit_store_upvalue(&mut self, slot: u32) {
        let val = self.stack.pop().expect("stack underflow store upvalue");
        self.push_inst(IrInst::StoreUpvalue { slot, value: val });
    }

    pub fn emit_binop(&mut self, op: u8) {
        let right = self.stack.pop().expect("binop right");
        let left = self.stack.pop().expect("binop left");
        let ir_op = match op {
            OP_ADD => IrBinOp::Add,
            OP_SUBTRACT => IrBinOp::Sub,
            OP_MULTIPLY => IrBinOp::Mul,
            OP_DIVIDE => IrBinOp::Div,
            OP_EQUALS => IrBinOp::Eq,
            OP_NOT_EQUALS => IrBinOp::Ne,
            OP_GREATER => IrBinOp::Gt,
            OP_LESS => IrBinOp::Lt,
            OP_GREATER_EQUAL => IrBinOp::Ge,
            OP_LESS_EQUAL => IrBinOp::Le,
            _ => IrBinOp::Add,
        };
        let r = self.fresh();
        self.push_inst(IrInst::BinOp {
            result: r,
            op: ir_op,
            left,
            right,
        });
    }

    pub fn emit_not(&mut self) {
        let v = self.stack.pop().expect("not");
        let r = self.fresh();
        self.push_inst(IrInst::Not { result: r, value: v });
    }

    pub fn emit_print(&mut self) {
        let v = self.stack.pop().expect("print");
        self.push_inst(IrInst::Print { value: v });
    }

    pub fn emit_pop(&mut self) {
        let v = self.stack.pop().expect("pop");
        self.push_inst(IrInst::Pop { value: v });
    }

    pub fn emit_input(&mut self, type_mask: u8) {
        let prompt = self.stack.pop().expect("input prompt");
        let r = self.fresh();
        self.push_inst(IrInst::Input {
            result: r,
            prompt,
            type_mask,
        });
    }

    pub fn emit_call(&mut self, n_args: u32) {
        let mut args: Vec<ValueId> = Vec::new();
        for _ in 0..n_args {
            args.push(self.stack.pop().expect("call arg"));
        }
        args.reverse();
        let callee = self.stack.pop().expect("call callee");
        let r = self.fresh();
        self.push_inst(IrInst::Call {
            result: r,
            callee,
            args,
        });
    }

    pub fn emit_return(&mut self) {
        let val = self.stack.pop();
        let func = &mut self.module.functions[self.current_func.0 as usize];
        func.block_mut(func.entry).term = Some(IrTerminator::Return { value: val });
    }

    pub fn emit_make_closure(
        &mut self,
        func_offset: u32,
        num_params: u32,
        captures: &[CaptureDesc],
    ) {
        let func_id = self.func_offsets.get(&func_offset).copied().unwrap_or_else(|| {
            let id = FunctionId(self.module.functions.len() as u32);
            let params: Vec<IrParam> = (0..num_params)
                .map(|i| IrParam {
                    name: format!("p{}", i),
                    local_index: i,
                })
                .collect();
            self.module.functions.push(IrFunction::new(None, params));
            self.module.functions[id.0 as usize].captures = captures.to_vec();
            self.func_offsets.insert(func_offset, id);
            id
        });
        let r = self.fresh();
        self.push_inst(IrInst::MakeClosure {
            result: r,
            func: func_id,
        });
    }

    pub fn begin_function(&mut self, params: &[String], captures: &[CaptureDesc]) -> FunctionId {
        let id = FunctionId(self.module.functions.len() as u32);
        let ir_params: Vec<IrParam> = params
            .iter()
            .enumerate()
            .map(|(i, n)| IrParam {
                name: n.clone(),
                local_index: i as u32,
            })
            .collect();
        let mut func = IrFunction::new(None, ir_params);
        func.captures = captures.to_vec();
        func.num_locals = params.len() as u32;
        self.module.functions.push(func);
        self.compiling_func = Some(id);
        self.current_func = id;
        self.stack.clear();
        id
    }

    pub fn end_function(&mut self) {
        self.compiling_func = None;
        self.current_func = FunctionId(0);
        self.stack.clear();
    }

    pub fn emit_loop_inc_less(&mut self, slot: u32, limit: i64) {
        self.push_inst(IrInst::LoopIncLess {
            global_idx: slot,
            limit,
        });
    }

    pub fn emit_loop_step_less(&mut self, slot: u32, limit: i64, step: i64) {
        self.push_inst(IrInst::LoopStepLess {
            global_idx: slot,
            limit,
            step,
        });
    }

    pub fn emit_build_array(&mut self, count: u32, mutable: bool) {
        let mut elements = Vec::new();
        for _ in 0..count {
            elements.push(self.stack.pop().expect("array elem"));
        }
        elements.reverse();
        let r = self.fresh();
        self.push_inst(IrInst::BuildArray {
            result: r,
            elements,
            mutable,
        });
    }

    pub fn emit_build_object(&mut self, count: u32, keys: &[u32]) {
        let mut field_values = Vec::new();
        for _ in 0..count {
            field_values.push(self.stack.pop().expect("object field"));
        }
        field_values.reverse();
        let r = self.fresh();
        self.push_inst(IrInst::BuildObject {
            result: r,
            field_values,
            field_keys: keys.to_vec(),
        });
    }

    pub fn emit_index_get(&mut self) {
        let idx = self.stack.pop().expect("index");
        let obj = self.stack.pop().expect("object");
        let r = self.fresh();
        self.push_inst(IrInst::IndexGet {
            result: r,
            object: obj,
            index: idx,
        });
    }

    pub fn emit_index_set(&mut self) {
        let val = self.stack.pop().expect("index set val");
        let idx = self.stack.pop().expect("index set idx");
        let obj = self.stack.pop().expect("index set obj");
        self.push_inst(IrInst::IndexSet {
            object: obj,
            index: idx,
            value: val,
        });
    }

    pub fn emit_type(&mut self) {
        let v = self.stack.pop().expect("type");
        let r = self.fresh();
        self.push_inst(IrInst::Type { result: r, value: v });
    }

    pub fn emit_wait(&mut self) {
        let v = self.stack.pop().expect("wait");
        self.push_inst(IrInst::Wait { value: v });
        let r = self.fresh();
        self.push_inst(IrInst::Null { result: r });
    }

    pub fn emit_array_push(&mut self) {
        let val = self.stack.pop().expect("push val");
        let arr = self.stack.pop().expect("push arr");
        self.push_inst(IrInst::ArrayPush { array: arr, value: val });
        let r = self.fresh();
        self.push_inst(IrInst::Null { result: r });
    }

    pub fn emit_array_pop(&mut self) {
        let arr = self.stack.pop().expect("pop arr");
        let r = self.fresh();
        self.push_inst(IrInst::ArrayPop { result: r, array: arr });
    }

    pub fn emit_object_get_const(&mut self, key_idx: u32) {
        let obj = self.stack.pop().expect("object get");
        let r = self.fresh();
        self.push_inst(IrInst::ObjectGetConst {
            result: r,
            object: obj,
            key_idx,
        });
    }

    pub fn emit_member_length(&mut self) {
        let obj = self.stack.pop().expect("length");
        let r = self.fresh();
        self.push_inst(IrInst::MemberLength { result: r, object: obj });
    }

    pub fn emit_object_set(&mut self) {
        let val = self.stack.pop().expect("oset val");
        let key = self.stack.pop().expect("oset key");
        let obj = self.stack.pop().expect("oset obj");
        self.push_inst(IrInst::ObjectSet {
            object: obj,
            key,
            value: val,
        });
    }

    pub fn emit_object_delete(&mut self) {
        let key = self.stack.pop().expect("odel key");
        let obj = self.stack.pop().expect("odel obj");
        self.push_inst(IrInst::ObjectDelete { object: obj, key });
    }

    pub fn emit_object_keys(&mut self) {
        let obj = self.stack.pop().expect("keys");
        let r = self.fresh();
        self.push_inst(IrInst::ObjectKeys { result: r, object: obj });
    }

    pub fn emit_is_array(&mut self) {
        let v = self.stack.pop().expect("is array");
        let r = self.fresh();
        self.push_inst(IrInst::IsArray { result: r, value: v });
    }

    pub fn emit_is_object(&mut self) {
        let v = self.stack.pop().expect("is object");
        let r = self.fresh();
        self.push_inst(IrInst::IsObject { result: r, value: v });
    }

    pub fn emit_path_cur_store(&mut self, slot: u32) {
        let v = self.stack.pop().expect("path cur");
        self.push_inst(IrInst::StorePathCur { slot, value: v });
    }

    pub fn emit_path_cur_load(&mut self) {
        let slot = 0; // patched by caller context via dedicated slot in inst
        let _ = slot;
    }

    pub fn emit_path_cur_load_slot(&mut self, slot: u32) {
        let r = self.fresh();
        self.push_inst(IrInst::LoadPathCur { result: r, slot });
    }

    pub fn emit_object_get_or_create_const(&mut self, key_idx: u32) {
        let obj = self.stack.pop().expect("getorcreate");
        let r = self.fresh();
        self.push_inst(IrInst::ObjectGetOrCreateConst {
            result: r,
            object: obj,
            key_idx,
        });
    }

    pub fn emit_object_get(&mut self) {
        let key = self.stack.pop().expect("oget key");
        let obj = self.stack.pop().expect("oget obj");
        let r = self.fresh();
        self.push_inst(IrInst::ObjectGet {
            result: r,
            object: obj,
            key,
        });
    }

    pub fn emit_object_get_or_create(&mut self) {
        let key = self.stack.pop().expect("ogoc key");
        let obj = self.stack.pop().expect("ogoc obj");
        let r = self.fresh();
        self.push_inst(IrInst::ObjectGetOrCreate {
            result: r,
            object: obj,
            key,
        });
    }

    pub fn emit_array_len(&mut self) {
        let arr = self.stack.pop().expect("len");
        let r = self.fresh();
        self.push_inst(IrInst::ArrayLen { result: r, array: arr });
    }

    pub fn record_function_offset(&mut self, offset: u32, func_id: FunctionId) {
        self.func_offsets.insert(offset, func_id);
    }

    pub fn finish(mut self) -> IrModule {
        self.module
    }
}
