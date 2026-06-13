//! Lift flat stack bytecode into linear IR (one basic block per function).

use std::collections::{HashMap, HashSet};

use crate::bytecode::*;

use super::inst::{IrBinOp, IrInst};
use super::types::{CaptureDesc, IrFunction, IrModule, IrParam};
use super::value::{FunctionId, ValueId};

struct ClosureMeta {
    offset: u32,
    end: u32,
    num_params: u32,
    captures: Vec<CaptureDesc>,
}

struct LiftCtx {
    next_val: u32,
    stack: Vec<ValueId>,
    offset_to_fid: HashMap<u32, FunctionId>,
}

impl LiftCtx {
    fn fresh(&mut self) -> ValueId {
        let v = ValueId(self.next_val);
        self.next_val += 1;
        v
    }

    fn pop(&mut self) -> ValueId {
        self.stack.pop().expect("lift stack underflow")
    }
}

pub fn lift_bytecode(bytecode: &Bytecode) -> IrModule {
    let closures = discover_closures(bytecode);
    let skip_ranges: Vec<(usize, usize)> = closures
        .iter()
        .map(|c| (c.offset as usize, c.end as usize))
        .collect();

    let mut module = IrModule {
        globals: Vec::new(),
        global_map: HashMap::new(),
        constants: bytecode.constants.clone(),
        functions: Vec::new(),
        entry: FunctionId(0),
        fused_loops: Vec::new(),
    };

    let entry_fn = IrFunction::new(Some("<module>".to_string()), Vec::new());
    module.functions.push(entry_fn);

    let mut ctx = LiftCtx {
        next_val: 0,
        stack: Vec::new(),
        offset_to_fid: HashMap::new(),
    };

    for (i, meta) in closures.iter().enumerate() {
        let fid = FunctionId(i as u32 + 1);
        ctx.offset_to_fid.insert(meta.offset, fid);
        let params: Vec<IrParam> = (0..meta.num_params)
            .map(|j| IrParam {
                name: format!("p{}", j),
                local_index: j,
            })
            .collect();
        let mut func = IrFunction::new(None, params);
        func.captures = meta.captures.clone();
        func.num_locals = meta.num_params;
        func.lift_start = meta.offset;
        lift_range(
            bytecode,
            meta.offset as usize,
            meta.end as usize,
            &mut ctx,
            func.block_mut(func.entry),
        );
        module.functions.push(func);
    }

    module.functions[0].lift_start = 0;
    let entry_block = module.functions[0].entry;
    lift_entry(
        bytecode,
        &skip_ranges,
        &mut ctx,
        module.functions[0].block_mut(entry_block),
    );

    while module.globals.len() < bytecode.num_globals {
        let i = module.globals.len();
        module.globals.push(crate::ir::types::IrGlobal {
            name: format!("g{}", i),
            index: i as u32,
        });
        module.global_map.insert(format!("g{}", i), i as u32);
    }

    module
}

fn lift_entry(
    bytecode: &Bytecode,
    skip: &[(usize, usize)],
    ctx: &mut LiftCtx,
    block: &mut super::inst::IrBlock,
) {
    let mut ip = 0usize;
    while ip < bytecode.code.len() {
        if let Some(&(start, end)) = skip.iter().find(|&&(s, _)| s == ip) {
            ip = end;
            continue;
        }
        ip = lift_one(bytecode, &mut ip, ctx, block);
    }
}

fn lift_range(
    bytecode: &Bytecode,
    start: usize,
    end: usize,
    ctx: &mut LiftCtx,
    block: &mut super::inst::IrBlock,
) {
    let mut ip = start;
    while ip < end {
        ip = lift_one(bytecode, &mut ip, ctx, block);
    }
}

fn lift_one(
    bytecode: &Bytecode,
    ip: &mut usize,
    ctx: &mut LiftCtx,
    block: &mut super::inst::IrBlock,
) -> usize {
    let insn_start = *ip;
    let op = bytecode.code[*ip];
    *ip += 1;

    let inst = match op {
        OP_CONSTANT => {
            let idx = read_u32(bytecode, ip);
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Const { result: r, idx }
        }
        OP_NULL => {
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Null { result: r }
        }
        OP_LOAD_GLOBAL => {
            let slot = read_u32(bytecode, ip);
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::LoadGlobal { result: r, slot }
        }
        OP_STORE_GLOBAL => {
            let slot = read_u32(bytecode, ip);
            let val = ctx.pop();
            IrInst::StoreGlobal { slot, value: val }
        }
        OP_INC_GLOBAL => {
            let slot = read_u32(bytecode, ip);
            IrInst::IncGlobal { slot }
        }
        OP_ADD_GLOBAL => {
            let slot = read_u32(bytecode, ip);
            let delta = read_i64(bytecode, ip);
            IrInst::AddGlobal { slot, delta }
        }
        OP_LOAD_LOCAL => {
            let slot = read_u32(bytecode, ip);
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::LoadLocal { result: r, slot }
        }
        OP_STORE_LOCAL => {
            let slot = read_u32(bytecode, ip);
            let val = ctx.pop();
            IrInst::StoreLocal { slot, value: val }
        }
        OP_LOAD_UPVALUE => {
            let slot = read_u32(bytecode, ip);
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::LoadUpvalue { result: r, slot }
        }
        OP_STORE_UPVALUE => {
            let slot = read_u32(bytecode, ip);
            let val = ctx.pop();
            IrInst::StoreUpvalue { slot, value: val }
        }
        OP_ADD | OP_SUBTRACT | OP_MULTIPLY | OP_DIVIDE | OP_EQUALS | OP_NOT_EQUALS
        | OP_GREATER | OP_LESS | OP_GREATER_EQUAL | OP_LESS_EQUAL => {
            let right = ctx.pop();
            let left = ctx.pop();
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
                _ => unreachable!(),
            };
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::BinOp {
                result: r,
                op: ir_op,
                left,
                right,
            }
        }
        OP_NOT => {
            let v = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Not { result: r, value: v }
        }
        OP_PRINT => IrInst::Print { value: ctx.pop() },
        OP_POP => IrInst::Pop { value: ctx.pop() },
        OP_JUMP => {
            let target = read_u32(bytecode, ip);
            IrInst::Jump { target_ip: target }
        }
        OP_JUMP_IF_FALSE => {
            let target = read_u32(bytecode, ip);
            let _cond = ctx.pop();
            IrInst::JumpIfFalse { target_ip: target }
        }
        OP_MAKE_CLOSURE => {
            let func_offset = read_u32(bytecode, ip);
            let _num_params = read_u32(bytecode, ip);
            let num_upvalues = read_u32(bytecode, ip) as usize;
            for _ in 0..num_upvalues {
                let _kind = bytecode.code[*ip];
                *ip += 1;
                read_u32(bytecode, ip);
            }
            let func_id = *ctx
                .offset_to_fid
                .get(&func_offset)
                .unwrap_or(&FunctionId(0));
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::MakeClosure {
                result: r,
                func: func_id,
            }
        }
        OP_CALL_FUNCTION => {
            let n_args = read_u32(bytecode, ip);
            let mut args = Vec::new();
            for _ in 0..n_args {
                args.push(ctx.pop());
            }
            args.reverse();
            let callee = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Call {
                result: r,
                callee,
                args,
            }
        }
        OP_RETURN => {
            let val = ctx.stack.pop();
            IrInst::Return { value: val }
        }
        OP_INPUT => {
            let type_mask = bytecode.code[*ip];
            *ip += 1;
            let prompt = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Input {
                result: r,
                prompt,
                type_mask,
            }
        }
        OP_BUILD_ARRAY => {
            let count = read_u32(bytecode, ip);
            let mutable = bytecode.code[*ip] != 0;
            *ip += 1;
            let mut elements = Vec::new();
            for _ in 0..count {
                elements.push(ctx.pop());
            }
            elements.reverse();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::BuildArray {
                result: r,
                elements,
                mutable,
            }
        }
        OP_INDEX_GET => {
            let idx = ctx.pop();
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::IndexGet {
                result: r,
                object: obj,
                index: idx,
            }
        }
        OP_INDEX_SET => {
            let val = ctx.pop();
            let idx = ctx.pop();
            let obj = ctx.pop();
            IrInst::IndexSet {
                object: obj,
                index: idx,
                value: val,
            }
        }
        OP_ARRAY_LEN => {
            let arr = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ArrayLen { result: r, array: arr }
        }
        OP_ARRAY_PUSH => {
            let val = ctx.pop();
            let arr = ctx.pop();
            let push = IrInst::ArrayPush { array: arr, value: val };
            block.push_inst(insn_start as u32, push);
            // Compiler emits OP_NULL after push
            let null_ip = *ip;
            if *ip < bytecode.code.len() && bytecode.code[*ip] == OP_NULL {
                *ip += 1;
                let r = ctx.fresh();
                ctx.stack.push(r);
                block.push_inst(null_ip as u32, IrInst::Null { result: r });
            }
            return *ip;
        }
        OP_ARRAY_POP => {
            let arr = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ArrayPop { result: r, array: arr }
        }
        OP_BUILD_OBJECT => {
            let count = read_u32(bytecode, ip);
            let mut field_keys = Vec::new();
            for _ in 0..count {
                field_keys.push(read_u32(bytecode, ip));
            }
            let mut field_values = Vec::new();
            for _ in 0..count {
                field_values.push(ctx.pop());
            }
            field_values.reverse();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::BuildObject {
                result: r,
                field_values,
                field_keys,
            }
        }
        OP_OBJECT_GET_CONST => {
            let key_idx = read_u32(bytecode, ip);
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ObjectGetConst {
                result: r,
                object: obj,
                key_idx,
            }
        }
        OP_OBJECT_GET => {
            let key = ctx.pop();
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ObjectGet {
                result: r,
                object: obj,
                key,
            }
        }
        OP_OBJECT_SET => {
            let val = ctx.pop();
            let key = ctx.pop();
            let obj = ctx.pop();
            IrInst::ObjectSet {
                object: obj,
                key,
                value: val,
            }
        }
        OP_OBJECT_DELETE => {
            let key = ctx.pop();
            let obj = ctx.pop();
            IrInst::ObjectDelete { object: obj, key }
        }
        OP_OBJECT_KEYS => {
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ObjectKeys { result: r, object: obj }
        }
        OP_OBJECT_GET_OR_CREATE_CONST => {
            let key_idx = read_u32(bytecode, ip);
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ObjectGetOrCreateConst {
                result: r,
                object: obj,
                key_idx,
            }
        }
        OP_OBJECT_GET_OR_CREATE => {
            let key = ctx.pop();
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ObjectGetOrCreate {
                result: r,
                object: obj,
                key,
            }
        }
        OP_IS_ARRAY => {
            let v = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::IsArray { result: r, value: v }
        }
        OP_IS_OBJECT => {
            let v = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::IsObject { result: r, value: v }
        }
        OP_MEMBER_LENGTH => {
            let obj = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::MemberLength { result: r, object: obj }
        }
        OP_FOR_IN_ARRAY_HEADER => {
            let arr_slot = read_u32(bytecode, ip);
            let i_slot = read_u32(bytecode, ip);
            let exit_ip = read_u32(bytecode, ip);
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::ForInArrayHeader {
                arr_slot,
                i_slot,
                exit_ip,
            }
        }
        OP_FOR_IN_ARRAY_NEXT => {
            let i_slot = read_u32(bytecode, ip);
            let loop_start_ip = read_u32(bytecode, ip);
            IrInst::ForInArrayNext {
                i_slot,
                loop_start_ip,
            }
        }
        OP_LOOP_INC_LESS => {
            let slot = read_u32(bytecode, ip);
            let limit = read_i64(bytecode, ip);
            IrInst::LoopIncLess {
                global_idx: slot,
                limit,
            }
        }
        OP_LOOP_STEP_LESS => {
            let slot = read_u32(bytecode, ip);
            let limit = read_i64(bytecode, ip);
            let step = read_i64(bytecode, ip);
            IrInst::LoopStepLess {
                global_idx: slot,
                limit,
                step,
            }
        }
        OP_LESS_CONST_JUMP_IF_FALSE => {
            let slot = read_u32(bytecode, ip);
            let limit = read_i64(bytecode, ip);
            let target = read_u32(bytecode, ip);
            IrInst::LessConstJumpIfFalse {
                global_idx: slot,
                limit,
                target_ip: target,
            }
        }
        OP_TYPE => {
            let v = ctx.pop();
            let r = ctx.fresh();
            ctx.stack.push(r);
            IrInst::Type { result: r, value: v }
        }
        OP_WAIT => {
            let v = ctx.pop();
            let wait = IrInst::Wait { value: v };
            block.push_inst(insn_start as u32, wait);
            let null_ip = *ip;
            if *ip < bytecode.code.len() && bytecode.code[*ip] == OP_NULL {
                *ip += 1;
                let r = ctx.fresh();
                ctx.stack.push(r);
                block.push_inst(null_ip as u32, IrInst::Null { result: r });
            }
            return *ip;
        }
        _ => {
            panic!("lift: unsupported opcode {} at ip {}", op, insn_start);
        }
    };

    block.push_inst(insn_start as u32, inst);
    *ip
}

fn discover_closures(bytecode: &Bytecode) -> Vec<ClosureMeta> {
    let mut metas = Vec::new();
    let mut seen = HashSet::new();
    let mut ip = 0usize;
    while ip < bytecode.code.len() {
        if bytecode.code[ip] == OP_MAKE_CLOSURE {
            ip += 1;
            let offset = read_u32_at(bytecode, ip);
            ip += 4;
            let num_params = read_u32_at(bytecode, ip);
            ip += 4;
            let num_upvalues = read_u32_at(bytecode, ip) as usize;
            ip += 4;
            let mut captures = Vec::new();
            for _ in 0..num_upvalues {
                let kind = bytecode.code[ip];
                ip += 1;
                let index = read_u32_at(bytecode, ip);
                ip += 4;
                captures.push(CaptureDesc { kind, index });
            }
            if seen.insert(offset) {
                let end = function_end(bytecode, offset);
                metas.push(ClosureMeta {
                    offset,
                    end,
                    num_params,
                    captures,
                });
            }
        } else {
            ip = next_insn_start(bytecode, ip);
        }
    }
    metas.sort_by_key(|m| m.offset);
    metas
}

fn function_end(bytecode: &Bytecode, start: u32) -> u32 {
    let s = start as usize;
    if s >= 5 && bytecode.code[s - 5] == OP_JUMP {
        read_u32_at(bytecode, s - 4)
    } else {
        bytecode.code.len() as u32
    }
}

fn next_insn_start(bytecode: &Bytecode, ip: usize) -> usize {
    let op = bytecode.code[ip];
    let mut next = ip + 1;
    match op {
        OP_CONSTANT
        | OP_LOAD_GLOBAL
        | OP_STORE_GLOBAL
        | OP_LOAD_LOCAL
        | OP_STORE_LOCAL
        | OP_LOAD_UPVALUE
        | OP_STORE_UPVALUE
        | OP_INC_GLOBAL
        | OP_JUMP
        | OP_JUMP_IF_FALSE => next += 4,
        OP_ADD_GLOBAL => next += 4 + 8,
        OP_LOOP_INC_LESS => next += 4 + 8,
        OP_LOOP_STEP_LESS => next += 4 + 8 + 8,
        OP_LESS_CONST_JUMP_IF_FALSE => next += 4 + 8 + 4,
        OP_MAKE_CLOSURE => {
            let func_offset = read_u32_at(bytecode, next);
            let _ = func_offset;
            next += 4;
            let _num_params = read_u32_at(bytecode, next);
            next += 4;
            let num_upvalues = read_u32_at(bytecode, next) as usize;
            next += 4;
            for _ in 0..num_upvalues {
                next += 1 + 4;
            }
        }
        OP_CALL_FUNCTION => next += 4,
        OP_INPUT => next += 1,
        OP_BUILD_ARRAY => next += 4 + 1,
        OP_BUILD_OBJECT => {
            let count = read_u32_at(bytecode, next) as usize;
            next += 4 + count * 4;
        }
        OP_OBJECT_GET_CONST | OP_OBJECT_GET_OR_CREATE_CONST => next += 4,
        OP_FOR_IN_ARRAY_HEADER => next += 4 + 4 + 4,
        OP_FOR_IN_ARRAY_NEXT => next += 4 + 4,
        OP_ARRAY_PUSH | OP_WAIT => next += 1,
        _ => {}
    }
    next
}

fn read_u32(bytecode: &Bytecode, ip: &mut usize) -> u32 {
    let v = read_u32_at(bytecode, *ip);
    *ip += 4;
    v
}

fn read_i64(bytecode: &Bytecode, ip: &mut usize) -> i64 {
    let v = Bytecode::read_i64(&bytecode.code, *ip);
    *ip += 8;
    v
}

fn read_u32_at(bytecode: &Bytecode, ip: usize) -> u32 {
    Bytecode::read_u32(&bytecode.code, ip)
}
