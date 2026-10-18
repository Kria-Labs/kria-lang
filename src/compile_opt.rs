use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOperator, Expression, Literal, Statement};

/// Names assigned anywhere in `body` (top-level statements only).
pub fn assigned_in_body(body: &[Statement]) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in body {
        assigned_in_statement(stmt, &mut names);
    }
    names
}

fn assigned_in_statement(stmt: &Statement, names: &mut HashSet<String>) {
    match stmt {
        Statement::Assignment { name, .. } => {
            names.insert(name.clone());
        }
        Statement::IndexAssign { .. } | Statement::PropertyAssign { .. } => {}
        Statement::If {
            branches,
            else_branch,
        } => {
            for (_, block) in branches {
                for s in block {
                    assigned_in_statement(s, names);
                }
            }
            if let Some(block) = else_branch {
                for s in block {
                    assigned_in_statement(s, names);
                }
            }
        }
        Statement::While { body, .. } => {
            for s in body {
                assigned_in_statement(s, names);
            }
        }
        Statement::ForIn {
            body,
            key_name,
            value_name,
            ..
        } => {
            names.insert(key_name.clone());
            if let Some(v) = value_name {
                names.insert(v.clone());
            }
            for s in body {
                assigned_in_statement(s, names);
            }
        }
        Statement::FunctionDef { name, body, .. } => {
            names.insert(name.clone());
            for s in body {
                assigned_in_statement(s, names);
            }
        }
        _ => {}
    }
}

/// `(object_ident, member)` pairs read via dot access in expressions under `body`.
pub fn hoistable_member_pairs(body: &[Statement]) -> Vec<(String, String)> {
    let assigned = assigned_in_body(body);
    let mut seen = HashSet::new();
    let mut pairs = Vec::new();
    for stmt in body {
        collect_member_pairs_stmt(stmt, &assigned, &mut seen, &mut pairs);
    }
    pairs
}

pub fn has_unknown_side_effects(body: &[Statement]) -> bool {
    body.iter().any(statement_has_unknown_side_effects)
}

fn statement_has_unknown_side_effects(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assignment { value, .. }
        | Statement::Print(value)
        | Statement::Return(Some(value))
        | Statement::Expression(value) => expression_has_side_effects(value),
        Statement::If {
            branches,
            else_branch,
        } => {
            branches.iter().any(|(condition, body)| {
                expression_has_side_effects(condition) || has_unknown_side_effects(body)
            }) || else_branch
                .as_ref()
                .is_some_and(|body| has_unknown_side_effects(body))
        }
        Statement::While { condition, body } => {
            expression_has_side_effects(condition) || has_unknown_side_effects(body)
        }
        Statement::ForIn { iterable, body, .. } => {
            expression_has_side_effects(iterable) || has_unknown_side_effects(body)
        }
        Statement::IndexAssign { .. }
        | Statement::PropertyAssign { .. }
        | Statement::FunctionDef { .. }
        | Statement::Import { .. } => true,
        Statement::Return(None) | Statement::Break | Statement::Continue => false,
    }
}

fn expression_has_side_effects(expr: &Expression) -> bool {
    match expr {
        Expression::FunctionCall { .. }
        | Expression::Call { .. }
        | Expression::Input { .. }
        | Expression::FunctionExpr { .. } => true,
        Expression::UnaryOp { expr, .. } => expression_has_side_effects(expr),
        Expression::BinaryOp { left, right, .. } => {
            expression_has_side_effects(left) || expression_has_side_effects(right)
        }
        Expression::Index { object, index } => {
            expression_has_side_effects(object) || expression_has_side_effects(index)
        }
        Expression::MemberAccess { object, .. } => expression_has_side_effects(object),
        Expression::Literal(Literal::Array { elements, .. }) => {
            elements.iter().any(expression_has_side_effects)
        }
        Expression::Literal(Literal::Object { fields }) => fields
            .iter()
            .any(|(_, value)| expression_has_side_effects(value)),
        Expression::Literal(_) | Expression::Identifier(_) => false,
    }
}

fn collect_member_pairs_stmt(
    stmt: &Statement,
    assigned: &HashSet<String>,
    seen: &mut HashSet<(String, String)>,
    pairs: &mut Vec<(String, String)>,
) {
    match stmt {
        Statement::Assignment { value, .. }
        | Statement::Print(value)
        | Statement::Return(Some(value))
        | Statement::Expression(value) => {
            collect_member_pairs_expr(value, assigned, seen, pairs);
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            for (cond, block) in branches {
                collect_member_pairs_expr(cond, assigned, seen, pairs);
                for s in block {
                    collect_member_pairs_stmt(s, assigned, seen, pairs);
                }
            }
            if let Some(block) = else_branch {
                for s in block {
                    collect_member_pairs_stmt(s, assigned, seen, pairs);
                }
            }
        }
        Statement::While { condition, body } => {
            collect_member_pairs_expr(condition, assigned, seen, pairs);
            for s in body {
                collect_member_pairs_stmt(s, assigned, seen, pairs);
            }
        }
        Statement::ForIn { iterable, body, .. } => {
            collect_member_pairs_expr(iterable, assigned, seen, pairs);
            for s in body {
                collect_member_pairs_stmt(s, assigned, seen, pairs);
            }
        }
        _ => {}
    }
}

fn collect_member_pairs_expr(
    expr: &Expression,
    assigned: &HashSet<String>,
    seen: &mut HashSet<(String, String)>,
    pairs: &mut Vec<(String, String)>,
) {
    match expr {
        Expression::MemberAccess { object, member } => {
            if let Expression::Identifier(obj) = object.as_ref() {
                if !assigned.contains(obj) {
                    let key = (obj.clone(), member.clone());
                    if seen.insert(key.clone()) {
                        pairs.push(key);
                    }
                }
            }
            collect_member_pairs_expr(object, assigned, seen, pairs);
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_member_pairs_expr(left, assigned, seen, pairs);
            collect_member_pairs_expr(right, assigned, seen, pairs);
        }
        Expression::UnaryOp { expr, .. } => {
            collect_member_pairs_expr(expr, assigned, seen, pairs);
        }
        Expression::Index { object, index } => {
            collect_member_pairs_expr(object, assigned, seen, pairs);
            collect_member_pairs_expr(index, assigned, seen, pairs);
        }
        Expression::FunctionCall { args, .. } => {
            for a in args {
                collect_member_pairs_expr(a, assigned, seen, pairs);
            }
        }
        Expression::Call { callee, args } => {
            collect_member_pairs_expr(callee, assigned, seen, pairs);
            for a in args {
                collect_member_pairs_expr(a, assigned, seen, pairs);
            }
        }
        _ => {}
    }
}

pub fn fold_binary_literal(
    left: &Expression,
    op: BinaryOperator,
    right: &Expression,
) -> Option<crate::vm::Value> {
    use crate::ast::Literal;
    use crate::vm::{apply_numeric_op, NumericOp, Value};
    use std::sync::Arc;

    let left_literal = match left {
        Expression::Literal(literal) => literal,
        _ => return None,
    };
    let right_literal = match right {
        Expression::Literal(literal) => literal,
        _ => return None,
    };

    let left_number = match left_literal {
        Literal::Number(value) => Some(Value::Number(*value)),
        Literal::Float(value) => Some(Value::Float(*value)),
        _ => None,
    };
    let right_number = match right_literal {
        Literal::Number(value) => Some(Value::Number(*value)),
        Literal::Float(value) => Some(Value::Float(*value)),
        _ => None,
    };
    if let (Some(left), Some(right)) = (&left_number, &right_number) {
        let numeric_op = match op {
            BinaryOperator::Add => NumericOp::Add,
            BinaryOperator::Subtract => NumericOp::Subtract,
            BinaryOperator::Multiply => NumericOp::Multiply,
            BinaryOperator::Divide => NumericOp::Divide,
            BinaryOperator::Equals => NumericOp::Equals,
            BinaryOperator::NotEquals => NumericOp::NotEquals,
            BinaryOperator::GreaterThan => NumericOp::Greater,
            BinaryOperator::LessThan => NumericOp::Less,
            BinaryOperator::GreaterOrEqual => NumericOp::GreaterEqual,
            BinaryOperator::LessOrEqual => NumericOp::LessEqual,
            BinaryOperator::And | BinaryOperator::Or => return None,
        };
        return apply_numeric_op(numeric_op, left, right).ok();
    }

    match (left_literal, right_literal) {
        (Literal::Boolean(l), Literal::Boolean(r)) => Some(Value::Boolean(match op {
            BinaryOperator::And => *l && *r,
            BinaryOperator::Or => *l || *r,
            _ => return None,
        })),
        (Literal::String(l), Literal::String(r)) if matches!(op, BinaryOperator::Add) => {
            let mut s = String::with_capacity(l.len() + r.len());
            s.push_str(l);
            s.push_str(r);
            Some(Value::String(crate::vm::StringValue::from(Arc::from(s))))
        }
        _ => None,
    }
}

/// Maps `(obj, member)` to hoisted global variable name.
pub fn hoist_global_names(pairs: &[(String, String)]) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for (i, (obj, member)) in pairs.iter().enumerate() {
        map.insert(
            (obj.clone(), member.clone()),
            format!("__hoist_{}_{}", i, member),
        );
    }
    map
}
