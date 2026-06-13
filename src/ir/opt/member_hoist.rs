use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOperator, Expression, Statement};

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
        Statement::If { branches, else_branch } => {
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
        Statement::ForIn { body, key_name, value_name, .. } => {
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
        Statement::If { branches, else_branch } => {
            for (_, block) in branches {
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
        Statement::While { body, condition } => {
            collect_member_pairs_expr(condition, assigned, seen, pairs);
            for s in body {
                collect_member_pairs_stmt(s, assigned, seen, pairs);
            }
        }
        Statement::ForIn {
            iterable,
            body,
            ..
        } => {
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

/// IR-level member hoist pass (no-op when hoisting done at lower time).
pub fn run(_module: &mut crate::ir::types::IrModule) {}
