use std::collections::HashMap;

use crate::vm::Value;

use super::super::inst::{IrBinOp, IrInst};
use super::super::types::IrModule;
use super::super::value::ValueId;

/// Fold constant BinOps in IR where both operands are Const.
pub fn run(module: &mut IrModule) {
    let constants = module.constants.clone();
    let base_len = constants.len();
    let mut extra_constants: Vec<Value> = Vec::new();

    for func in &mut module.functions {
        for block in &mut func.blocks {
            let mut const_map: HashMap<ValueId, u32> = HashMap::new();
            let mut out = Vec::with_capacity(block.insts.len());
            let mut out_ips = Vec::with_capacity(block.inst_ips.len());

            for (i, inst) in block.insts.drain(..).enumerate() {
                let ip = block.inst_ips.get(i).copied().unwrap_or(0);
                match &inst {
                    IrInst::Const { result, idx } => {
                        const_map.insert(*result, *idx);
                        out.push(inst);
                        out_ips.push(ip);
                    }
                    IrInst::BinOp {
                        result,
                        op,
                        left,
                        right,
                    } => {
                        if let (Some(&li), Some(&ri)) =
                            (const_map.get(left), const_map.get(right))
                        {
                            let li = li as usize;
                            let ri = ri as usize;
                            if li < constants.len() && ri < constants.len() {
                                if let Some(folded) = fold_const_binop(
                                    &constants[li],
                                    *op,
                                    &constants[ri],
                                ) {
                                let idx = (base_len + extra_constants.len()) as u32;
                                extra_constants.push(folded);
                                const_map.insert(*result, idx);
                                out.push(IrInst::Const {
                                    result: *result,
                                    idx,
                                });
                                out_ips.push(ip);
                                continue;
                                }
                            }
                        }
                        out.push(inst);
                        out_ips.push(ip);
                    }
                    other => {
                        if let Some(r) = other.result() {
                            const_map.remove(&r);
                        }
                        out.push(inst);
                        out_ips.push(ip);
                    }
                }
            }

            block.insts = out;
            block.inst_ips = out_ips;
        }
    }

    module.constants.extend(extra_constants);
}

fn fold_const_binop(left: &Value, op: IrBinOp, right: &Value) -> Option<Value> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => Some(Value::Number(match op {
            IrBinOp::Add => l + r,
            IrBinOp::Sub => l - r,
            IrBinOp::Mul => l * r,
            IrBinOp::Div => {
                if *r == 0 {
                    return None;
                }
                l / r
            }
            IrBinOp::Eq => return Some(Value::Boolean(l == r)),
            IrBinOp::Ne => return Some(Value::Boolean(l != r)),
            IrBinOp::Gt => return Some(Value::Boolean(l > r)),
            IrBinOp::Lt => return Some(Value::Boolean(l < r)),
            IrBinOp::Ge => return Some(Value::Boolean(l >= r)),
            IrBinOp::Le => return Some(Value::Boolean(l <= r)),
        })),
        (Value::Boolean(l), Value::Boolean(r)) => match op {
            IrBinOp::Eq => Some(Value::Boolean(l == r)),
            IrBinOp::Ne => Some(Value::Boolean(l != r)),
            _ => None,
        },
        (Value::String(l), Value::String(r)) if matches!(op, IrBinOp::Add) => {
            let mut s = String::with_capacity(l.len() + r.len());
            s.push_str(l);
            s.push_str(r);
            Some(Value::String(std::sync::Arc::from(s)))
        }
        _ => None,
    }
}
