//! Lift flat stack bytecode into linear SSA IR (one basic block per function).

use crate::bytecode::*;
use crate::vm::Value;

use super::inst::{IrBinOp, IrInst, IrTerminator};
use super::types::{IrFunction, IrModule, IrParam};
use super::value::{FunctionId, ValueId};

pub fn lift_bytecode(bytecode: &Bytecode) -> IrModule {
    let mut module = IrModule {
        globals: Vec::new(),
        global_map: std::collections::HashMap::new(),
        constants: bytecode.constants.clone(),
        functions: Vec::new(),
        entry: FunctionId(0),
        fused_loops: Vec::new(),
    };

    let entry = IrFunction::new(Some("<module>".to_string()), Vec::new());
    module.functions.push(entry);

    let mut ip = 0;
    let mut stack: Vec<ValueId> = Vec::new();
    let mut next_id = 0u32;

    let mut fresh = || {
        let v = ValueId(next_id);
        next_id += 1;
        v
    };

    let entry_fn = &mut module.functions[0];
    let block = entry_fn.entry;

    while ip < bytecode.code.len() {
        let op = bytecode.code[ip];
        ip += 1;
        let inst = match op {
            OP_CONSTANT => {
                let idx = read_u32(bytecode, &mut ip);
                let r = fresh();
                stack.push(r);
                IrInst::Const { result: r, idx }
            }
            OP_NULL => {
                let r = fresh();
                stack.push(r);
                IrInst::Null { result: r }
            }
            OP_LOAD_GLOBAL => {
                let slot = read_u32(bytecode, &mut ip);
                let r = fresh();
                stack.push(r);
                IrInst::LoadGlobal { result: r, slot }
            }
            OP_STORE_GLOBAL => {
                let slot = read_u32(bytecode, &mut ip);
                let val = stack.pop().unwrap();
                IrInst::StoreGlobal { slot, value: val }
            }
            OP_ADD | OP_SUBTRACT | OP_MULTIPLY | OP_DIVIDE | OP_EQUALS | OP_NOT_EQUALS
            | OP_GREATER | OP_LESS | OP_GREATER_EQUAL | OP_LESS_EQUAL => {
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();
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
                let r = fresh();
                stack.push(r);
                IrInst::BinOp {
                    result: r,
                    op: ir_op,
                    left,
                    right,
                }
            }
            OP_NOT => {
                let v = stack.pop().unwrap();
                let r = fresh();
                stack.push(r);
                IrInst::Not { result: r, value: v }
            }
            OP_PRINT => {
                let v = stack.pop().unwrap();
                IrInst::Print { value: v }
            }
            OP_POP => {
                let v = stack.pop().unwrap();
                IrInst::Pop { value: v }
            }
            OP_JUMP => {
                let _target = read_u32(bytecode, &mut ip);
                continue;
            }
            OP_JUMP_IF_FALSE => {
                let _target = read_u32(bytecode, &mut ip);
                let _cond = stack.pop().unwrap();
                continue;
            }
            OP_INC_GLOBAL => {
                let slot = read_u32(bytecode, &mut ip);
                IrInst::IncGlobal { slot }
            }
            OP_ADD_GLOBAL => {
                let slot = read_u32(bytecode, &mut ip);
                let delta = read_i64(bytecode, &mut ip);
                IrInst::AddGlobal { slot, delta }
            }
            OP_LOOP_INC_LESS => {
                let slot = read_u32(bytecode, &mut ip);
                let limit = read_i64(bytecode, &mut ip);
                IrInst::LoopIncLess {
                    global_idx: slot,
                    limit,
                }
            }
            OP_LOOP_STEP_LESS => {
                let slot = read_u32(bytecode, &mut ip);
                let limit = read_i64(bytecode, &mut ip);
                let step = read_i64(bytecode, &mut ip);
                IrInst::LoopStepLess {
                    global_idx: slot,
                    limit,
                    step,
                }
            }
            OP_RETURN => {
                let val = stack.pop();
                entry_fn.block_mut(block).term = Some(IrTerminator::Return { value: val });
                break;
            }
            OP_TYPE => {
                let v = stack.pop().unwrap();
                let r = fresh();
                stack.push(r);
                IrInst::Type { result: r, value: v }
            }
            OP_WAIT => {
                let v = stack.pop().unwrap();
                stack.push(fresh()); // null pushed by compiler
                IrInst::Wait { value: v }
            }
            _ => {
                // Skip unknown/advanced opcodes in lift (fallback: keep original bytecode path)
                ip = bytecode.code.len();
                break;
            }
        };
        entry_fn.block_mut(block).insts.push(inst);
    }

    module
}

fn read_u32(bytecode: &Bytecode, ip: &mut usize) -> u32 {
    let v = Bytecode::read_u32(&bytecode.code, *ip);
    *ip += 4;
    v
}

fn read_i64(bytecode: &Bytecode, ip: &mut usize) -> i64 {
    let v = Bytecode::read_i64(&bytecode.code, *ip);
    *ip += 8;
    v
}

#[allow(dead_code)]
fn sync_globals(module: &mut IrModule, num_globals: usize) {
    for i in 0..num_globals {
        let name = format!("g{}", i);
        module.resolve_global(&name);
    }
}
