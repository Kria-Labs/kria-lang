use crate::ast::{BinaryOperator, Expression, Literal};
use crate::vm::Value;
use std::sync::Arc;

pub fn fold_binary_literal(
    left: &Expression,
    op: BinaryOperator,
    right: &Expression,
) -> Option<Value> {
    let (Literal::Number(l), Literal::Number(r)) = (
        match left {
            Expression::Literal(l) => l,
            _ => return fold_non_numeric(left, op, right),
        },
        match right {
            Expression::Literal(l) => l,
            _ => return fold_non_numeric(left, op, right),
        },
    ) else {
        return fold_non_numeric(left, op, right);
    };

    Some(Value::Number(match op {
        BinaryOperator::Add => l + r,
        BinaryOperator::Subtract => l - r,
        BinaryOperator::Multiply => l * r,
        BinaryOperator::Divide => {
            if *r == 0 {
                return None;
            }
            l / r
        }
        BinaryOperator::Equals => return Some(Value::Boolean(l == r)),
        BinaryOperator::NotEquals => return Some(Value::Boolean(l != r)),
        BinaryOperator::GreaterThan => return Some(Value::Boolean(l > r)),
        BinaryOperator::LessThan => return Some(Value::Boolean(l < r)),
        BinaryOperator::GreaterOrEqual => return Some(Value::Boolean(l >= r)),
        BinaryOperator::LessOrEqual => return Some(Value::Boolean(l <= r)),
        BinaryOperator::And | BinaryOperator::Or => return None,
    }))
}

fn fold_non_numeric(
    left: &Expression,
    op: BinaryOperator,
    right: &Expression,
) -> Option<Value> {
    match (left, right) {
        (Expression::Literal(Literal::Boolean(l)), Expression::Literal(Literal::Boolean(r))) => {
            Some(Value::Boolean(match op {
                BinaryOperator::And => *l && *r,
                BinaryOperator::Or => *l || *r,
                _ => return None,
            }))
        }
        (Expression::Literal(Literal::String(l)), Expression::Literal(Literal::String(r)))
            if matches!(op, BinaryOperator::Add) =>
        {
            let mut s = String::with_capacity(l.len() + r.len());
            s.push_str(l);
            s.push_str(r);
            Some(Value::String(Arc::from(s)))
        }
        _ => None,
    }
}

use super::super::types::IrModule;

/// Fold constant BinOps in IR where both operands are Const (placeholder pass).
pub fn run(_module: &mut IrModule) {}
