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
                    IrInst::FoldedBinOp { result, idx, .. } => {
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
                        if let (Some(&li), Some(&ri)) = (const_map.get(left), const_map.get(right))
                        {
                            let li = li as usize;
                            let ri = ri as usize;
                            let left_value = constants
                                .get(li)
                                .or_else(|| extra_constants.get(li.saturating_sub(base_len)))
                                .cloned();
                            let right_value = constants
                                .get(ri)
                                .or_else(|| extra_constants.get(ri.saturating_sub(base_len)))
                                .cloned();
                            if let (Some(folded_left), Some(folded_right)) =
                                (left_value, right_value)
                            {
                                if let Some(folded) =
                                    fold_const_binop(&folded_left, *op, &folded_right)
                                {
                                    let idx = (base_len + extra_constants.len()) as u32;
                                    extra_constants.push(folded);
                                    const_map.insert(*result, idx);
                                    out.push(IrInst::FoldedBinOp {
                                        result: *result,
                                        idx,
                                        left: *left,
                                        right: *right,
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
    use crate::vm::{apply_numeric_op, NumericOp};

    let numeric_op = match op {
        IrBinOp::Add => NumericOp::Add,
        IrBinOp::Sub => NumericOp::Subtract,
        IrBinOp::Mul => NumericOp::Multiply,
        IrBinOp::Div => NumericOp::Divide,
        IrBinOp::Eq => NumericOp::Equals,
        IrBinOp::Ne => NumericOp::NotEquals,
        IrBinOp::Gt => NumericOp::Greater,
        IrBinOp::Lt => NumericOp::Less,
        IrBinOp::Ge => NumericOp::GreaterEqual,
        IrBinOp::Le => NumericOp::LessEqual,
    };
    if matches!(left, Value::Number(_) | Value::Float(_))
        && matches!(right, Value::Number(_) | Value::Float(_))
    {
        return apply_numeric_op(numeric_op, left, right).ok();
    }

    match (left, right) {
        (Value::Boolean(l), Value::Boolean(r)) => match op {
            IrBinOp::Eq => Some(Value::Boolean(l == r)),
            IrBinOp::Ne => Some(Value::Boolean(l != r)),
            _ => None,
        },
        (Value::String(l), Value::String(r)) if matches!(op, IrBinOp::Add) => {
            let mut s = String::with_capacity(l.len() + r.len());
            s.push_str(l.as_str());
            s.push_str(r.as_str());
            Some(Value::String(crate::vm::StringValue::from(
                std::sync::Arc::from(s),
            )))
        }
        _ => None,
    }
}
