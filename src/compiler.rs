use std::collections::HashMap;
use crate::ast::{Expression, Statement, Literal, BinaryOperator, UnaryOperator};
use crate::bytecode::*;
use crate::compile_opt::{fold_binary_literal, hoist_global_names, hoistable_member_pairs};
use crate::vm::Value;
use std::sync::Arc;

struct UpvalueCapture {
    kind: u8,
    index: u32,
}

struct CompileScope {
    locals: HashMap<String, usize>,
    upvalue_names: Vec<String>,
    captures: Vec<UpvalueCapture>,
}

#[derive(Clone)]
enum VarResolution {
    Local(usize),
    Upvalue(usize),
    Global(usize),
}

struct LoopContext {
    #[allow(dead_code)]
    start_pos: usize,
    continue_jumps: Vec<usize>,  // Positions of continue jumps to be patched
    exit_jumps: Vec<usize>,      // Positions of break jumps to be patched
}

pub struct Compiler {
    bytecode: Bytecode,
    globals: HashMap<String, usize>,
    scope_stack: Vec<CompileScope>,
    for_loop_counter: usize,
    path_counter: usize,
    loop_stack: Vec<LoopContext>,
    /// import alias -> exported function name -> global index
    import_bindings: HashMap<String, HashMap<String, usize>>,
    /// Loop-invariant hoisting: (obj, member) -> temp global name
    member_hoist: HashMap<(String, String), String>,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            bytecode: Bytecode::new(),
            globals: HashMap::new(),
            scope_stack: Vec::new(),
            for_loop_counter: 0,
            path_counter: 0,
            loop_stack: Vec::new(),
            import_bindings: HashMap::new(),
            member_hoist: HashMap::new(),
        }
    }

    pub fn begin_module(&mut self) {
        self.import_bindings.clear();
    }

    pub fn bind_import(
        &mut self,
        alias: &str,
        exports: &HashMap<String, usize>,
    ) -> Result<(), String> {
        if self.import_bindings.contains_key(alias) {
            return Err(format!("Duplicate import alias '{}'", alias));
        }
        self.import_bindings.insert(alias.to_string(), exports.clone());
        Ok(())
    }

    pub fn compile_module(
        &mut self,
        statements: &[Statement],
    ) -> Result<HashMap<String, usize>, String> {
        let mut exports = HashMap::new();
        for statement in statements {
            match statement {
                Statement::Import { .. } => {}
                Statement::FunctionDef {
                    name,
                    params,
                    body,
                    exported,
                } => {
                    let (func_offset, num_params, captures) =
                        self.compile_function(params, body)?;
                    let global_idx = self.resolve_global(name);
                    self.emit_make_closure(func_offset, num_params, &captures);
                    self.emit_opcode(OP_STORE_GLOBAL);
                    self.emit_u32(global_idx as u32);
                    if *exported {
                        exports.insert(name.clone(), global_idx);
                    }
                }
                _ => self.compile_statement(statement, false)?,
            }
        }
        self.bytecode.num_globals = self.globals.len();
        Ok(exports)
    }

    pub fn finish_bytecode(mut self) -> Bytecode {
        self.bytecode.num_globals = self.globals.len();
        self.bytecode
    }

    pub fn bytecode(&self) -> &Bytecode {
        &self.bytecode
    }

    pub fn compile(mut self, statements: &[Statement]) -> Result<Bytecode, String> {
        for statement in statements {
            self.compile_statement(statement, false)?;
        }
        self.bytecode.num_globals = self.globals.len();
        Ok(self.bytecode)
    }

    /// Append REPL input to accumulated bytecode. Last bare expression is auto-printed.
    pub fn compile_repl(&mut self, statements: &[Statement]) -> Result<(), String> {
        let n = statements.len();
        for (i, statement) in statements.iter().enumerate() {
            let repl_print_expr =
                i + 1 == n && matches!(statement, Statement::Expression(_));
            self.compile_statement(statement, repl_print_expr)?;
        }
        self.bytecode.num_globals = self.globals.len();
        Ok(())
    }

    fn emit_opcode(&mut self, op: u8) -> usize {
        self.bytecode.emit_byte(op)
    }

    fn emit_u32(&mut self, value: u32) -> usize {
        self.bytecode.emit_u32(value)
    }

    fn emit_i64(&mut self, value: i64) -> usize {
        self.bytecode.emit_i64(value)
    }

    fn patch_u32(&mut self, pos: usize, value: u32) {
        self.bytecode.patch_u32(pos, value);
    }

    fn emit_constant(&mut self, value: Value) {
        let idx = self.bytecode.add_constant(value);
        self.emit_opcode(OP_CONSTANT);
        self.emit_u32(idx);
    }

    fn emit_load_var(&mut self, resolved: VarResolution) -> Result<(), String> {
        match resolved {
            VarResolution::Local(index) => {
                self.emit_opcode(OP_LOAD_LOCAL);
                self.emit_u32(index as u32);
            }
            VarResolution::Upvalue(index) => {
                self.emit_opcode(OP_LOAD_UPVALUE);
                self.emit_u32(index as u32);
            }
            VarResolution::Global(index) => {
                self.emit_opcode(OP_LOAD_GLOBAL);
                self.emit_u32(index as u32);
            }
        }
        Ok(())
    }

    fn emit_store_var(&mut self, resolved: VarResolution) -> Result<(), String> {
        match resolved {
            VarResolution::Local(index) => {
                self.emit_opcode(OP_STORE_LOCAL);
                self.emit_u32(index as u32);
            }
            VarResolution::Upvalue(index) => {
                self.emit_opcode(OP_STORE_UPVALUE);
                self.emit_u32(index as u32);
            }
            VarResolution::Global(index) => {
                self.emit_opcode(OP_STORE_GLOBAL);
                self.emit_u32(index as u32);
            }
        }
        Ok(())
    }

    fn emit_make_closure(&mut self, func_offset: u32, num_params: u32, captures: &[UpvalueCapture]) {
        self.emit_opcode(OP_MAKE_CLOSURE);
        self.emit_u32(func_offset);
        self.emit_u32(num_params);
        self.emit_u32(captures.len() as u32);
        for cap in captures {
            self.bytecode.emit_byte(cap.kind);
            self.emit_u32(cap.index);
        }
    }

    fn compile_function(
        &mut self,
        params: &[String],
        body: &[Statement],
    ) -> Result<(u32, u32, Vec<UpvalueCapture>), String> {
        self.emit_opcode(OP_JUMP);
        let skip_pos = self.emit_u32(0);

        let func_offset = self.bytecode.code.len() as u32;

        self.scope_stack.push(CompileScope {
            locals: HashMap::new(),
            upvalue_names: Vec::new(),
            captures: Vec::new(),
        });

        for (i, param) in params.iter().enumerate() {
            self.scope_stack
                .last_mut()
                .unwrap()
                .locals
                .insert(param.clone(), i);
        }

        self.compile_block(body)?;

        if !matches!(body.last(), Some(Statement::Return(_))) {
            self.emit_opcode(OP_NULL);
            self.emit_opcode(OP_RETURN);
        }

        let scope = self.scope_stack.pop().unwrap();
        let captures = scope.captures;

        let after_func = self.bytecode.code.len();
        self.patch_u32(skip_pos, after_func as u32);

        Ok((func_offset, params.len() as u32, captures))
    }

    fn compile_statement(&mut self, statement: &Statement, repl_expr_print: bool) -> Result<(), String> {
        match statement {
            Statement::Assignment { name, value } => {
                if self.compile_special_assignment(name, value)?.is_some() {
                    return Ok(());
                }
                let resolved = self.resolve_identifier(name)?;
                self.compile_expression(value)?;
                self.emit_store_var(resolved)?;
            }
            Statement::Print(expr) => {
                self.compile_expression(expr)?;
                self.emit_opcode(OP_PRINT);
            }
            Statement::If { branches, else_branch } => {
                let mut jump_to_end_positions = Vec::new();

                for (i, (condition, branch)) in branches.iter().enumerate() {
                    self.compile_expression(condition)?;
                    self.emit_opcode(OP_JUMP_IF_FALSE);
                    let jump_to_next_pos = self.emit_u32(0);
                    self.compile_block(branch)?;
                    self.emit_opcode(OP_JUMP);
                    let jump_over_rest_pos = self.emit_u32(0);
                    jump_to_end_positions.push(jump_over_rest_pos);

                    let next_location = self.bytecode.code.len();
                    self.patch_u32(jump_to_next_pos, next_location as u32);

                    if i == branches.len() - 1 {
                        break;
                    }
                }

                if let Some(else_branch) = else_branch {
                    self.compile_block(else_branch)?;
                }

                let end_location = self.bytecode.code.len();
                for pos in jump_to_end_positions {
                    self.patch_u32(pos, end_location as u32);
                }
            }
            Statement::While { condition, body } => {
                if let Some(_) = self.try_compile_combined_while(condition, body)? {
                    return Ok(());
                }

                self.emit_loop_hoists(body)?;
                let loop_start = self.bytecode.code.len();
                self.loop_stack.push(LoopContext {
                    start_pos: loop_start,
                    continue_jumps: Vec::new(),
                    exit_jumps: Vec::new(),
                });

                self.compile_expression(condition)?;
                self.emit_opcode(OP_JUMP_IF_FALSE);
                let exit_jump_pos = self.emit_u32(0);
                self.compile_block(body)?;
                self.emit_opcode(OP_JUMP);
                self.emit_u32(loop_start as u32);
                let loop_end = self.bytecode.code.len();

                let loop_ctx = self.loop_stack.pop().unwrap();
                self.patch_u32(exit_jump_pos, loop_end as u32);
                for jump_pos in loop_ctx.exit_jumps {
                    self.patch_u32(jump_pos, loop_end as u32);
                }
                for jump_pos in loop_ctx.continue_jumps {
                    self.patch_u32(jump_pos, loop_start as u32);
                }
                self.member_hoist.clear();
            }
            Statement::FunctionDef {
                name,
                params,
                body,
                exported: _,
            } => {
                let (func_offset, num_params, captures) = self.compile_function(params, body)?;

                let global_idx = self.resolve_global(name);
                self.emit_make_closure(func_offset, num_params, &captures);
                self.emit_opcode(OP_STORE_GLOBAL);
                self.emit_u32(global_idx as u32);
            }
            Statement::Import { .. } => {}
            Statement::Return(expr) => {
                match expr {
                    Some(e) => self.compile_expression(e)?,
                    None => {
                        self.emit_opcode(OP_NULL);
                    }
                }
                self.emit_opcode(OP_RETURN);
            }
            Statement::IndexAssign { object, index, value } => {
                self.compile_expression(object)?;
                self.compile_expression(index)?;
                self.compile_expression(value)?;
                self.emit_opcode(OP_INDEX_SET);
            }
            Statement::ForIn {
                key_name,
                value_name,
                iterable,
                body,
            } => {
                self.compile_for_in(key_name, value_name.as_deref(), iterable, body)?;
            }
            Statement::PropertyAssign { target, value } => {
                self.compile_property_assign(target, value)?;
            }
            Statement::Break => {
                if self.loop_stack.is_empty() {
                    return Err("break can only be used inside a loop".to_string());
                }
                self.emit_opcode(OP_JUMP);
                let jump_pos = self.emit_u32(0);
                self.loop_stack.last_mut().unwrap().exit_jumps.push(jump_pos);
            }
            Statement::Continue => {
                if self.loop_stack.is_empty() {
                    return Err("continue can only be used inside a loop".to_string());
                }
                self.emit_opcode(OP_JUMP);
                let jump_pos = self.emit_u32(0);
                self.loop_stack.last_mut().unwrap().continue_jumps.push(jump_pos);
            }
            Statement::Expression(expr) => {
                self.compile_expression(expr)?;
                if repl_expr_print {
                    self.emit_opcode(OP_PRINT);
                } else {
                    self.emit_opcode(OP_POP);
                }
            }
        }

        Ok(())
    }

    fn compile_for_in(
        &mut self,
        key_name: &str,
        value_name: Option<&str>,
        iterable: &Expression,
        body: &[Statement],
    ) -> Result<(), String> {
        match value_name {
            None => self.compile_for_in_array(key_name, iterable, body),
            Some(val_name) => self.compile_for_in_object(key_name, val_name, iterable, body),
        }
    }

    fn emit_loop_hoists(&mut self, body: &[Statement]) -> Result<(), String> {
        let pairs = hoistable_member_pairs(body);
        if pairs.is_empty() {
            return Ok(());
        }
        let hoist_map = hoist_global_names(&pairs);
        for (obj, member) in &pairs {
            self.compile_expression(&Expression::MemberAccess {
                object: Box::new(Expression::Identifier(obj.clone())),
                member: member.clone(),
            })?;
            let temp = hoist_map.get(&(obj.clone(), member.clone())).unwrap().clone();
            let idx = self.resolve_global(&temp);
            self.emit_opcode(OP_STORE_GLOBAL);
            self.emit_u32(idx as u32);
        }
        self.member_hoist = hoist_map;
        Ok(())
    }

    fn compile_for_in_array(
        &mut self,
        item_name: &str,
        iterable: &Expression,
        body: &[Statement],
    ) -> Result<(), String> {
        let loop_id = self.for_loop_counter;
        self.for_loop_counter += 1;

        let arr_name = format!("__for_arr_{}", loop_id);
        let i_name = format!("__for_i_{}", loop_id);

        self.compile_expression(iterable)?;
        let arr_idx = self.resolve_global(&arr_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(arr_idx as u32);

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(arr_idx as u32);
        self.emit_opcode(OP_IS_ARRAY);
        self.emit_opcode(OP_JUMP_IF_FALSE);
        let type_err = self.emit_u32(0);

        self.emit_constant(Value::Number(0));
        let i_idx = self.resolve_global(&i_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(i_idx as u32);

        let loop_start = self.bytecode.code.len();
        self.loop_stack.push(LoopContext {
            start_pos: loop_start,
            continue_jumps: Vec::new(),
            exit_jumps: Vec::new(),
        });

        let exit_jump_placeholder = self.bytecode.code.len();
        self.emit_opcode(OP_FOR_IN_ARRAY_HEADER);
        self.emit_u32(arr_idx as u32);
        self.emit_u32(i_idx as u32);
        self.emit_u32(0); // patched to loop_end

        let item_resolved = self.resolve_identifier(item_name)?;
        self.emit_store_var(item_resolved)?;

        self.compile_block(body)?;

        let continue_pos = self.bytecode.code.len();

        self.emit_opcode(OP_FOR_IN_ARRAY_NEXT);
        self.emit_u32(i_idx as u32);
        self.emit_u32(loop_start as u32);

        let loop_end = self.bytecode.code.len();
        self.patch_u32(exit_jump_placeholder + 9, loop_end as u32);
        let loop_ctx = self.loop_stack.pop().unwrap();
        for jump_pos in loop_ctx.exit_jumps {
            self.patch_u32(jump_pos, loop_end as u32);
        }
        for jump_pos in loop_ctx.continue_jumps {
            self.patch_u32(jump_pos, continue_pos as u32);
        }

        self.emit_opcode(OP_JUMP);
        let skip_err = self.emit_u32(0);
        self.patch_u32(type_err, self.bytecode.code.len() as u32);
        self.emit_constant(Value::String(Arc::from(
            "for item in ... requires an array iterable",
        )));
        self.emit_opcode(OP_PRINT);
        self.emit_opcode(OP_NULL);
        self.patch_u32(skip_err, self.bytecode.code.len() as u32);

        Ok(())
    }

    fn compile_for_in_object(
        &mut self,
        key_name: &str,
        value_name: &str,
        iterable: &Expression,
        body: &[Statement],
    ) -> Result<(), String> {
        let loop_id = self.for_loop_counter;
        self.for_loop_counter += 1;

        let obj_name = format!("__for_obj_{}", loop_id);
        let keys_name = format!("__for_keys_{}", loop_id);
        let i_name = format!("__for_i_{}", loop_id);

        self.compile_expression(iterable)?;
        let obj_idx = self.resolve_global(&obj_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(obj_idx as u32);

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(obj_idx as u32);
        self.emit_opcode(OP_IS_OBJECT);
        self.emit_opcode(OP_JUMP_IF_FALSE);
        let type_err = self.emit_u32(0);

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(obj_idx as u32);
        self.emit_opcode(OP_OBJECT_KEYS);
        let keys_idx = self.resolve_global(&keys_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(keys_idx as u32);

        self.emit_constant(Value::Number(0));
        let i_idx = self.resolve_global(&i_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(i_idx as u32);

        let loop_start = self.bytecode.code.len();
        self.loop_stack.push(LoopContext {
            start_pos: loop_start,
            continue_jumps: Vec::new(),
            exit_jumps: Vec::new(),
        });

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(i_idx as u32);
        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(keys_idx as u32);
        self.emit_opcode(OP_ARRAY_LEN);
        self.emit_opcode(OP_LESS);
        self.emit_opcode(OP_JUMP_IF_FALSE);
        let exit_jump = self.emit_u32(0);

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(keys_idx as u32);
        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(i_idx as u32);
        self.emit_opcode(OP_INDEX_GET);
        let key_resolved = self.resolve_identifier(key_name)?;
        self.emit_store_var(key_resolved.clone())?;

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(obj_idx as u32);
        self.emit_load_var(key_resolved)?;
        self.emit_opcode(OP_OBJECT_GET);
        let val_resolved = self.resolve_identifier(value_name)?;
        self.emit_store_var(val_resolved)?;

        self.compile_block(body)?;

        let continue_pos = self.bytecode.code.len();

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(i_idx as u32);
        self.emit_constant(Value::Number(1));
        self.emit_opcode(OP_ADD);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(i_idx as u32);

        self.emit_opcode(OP_JUMP);
        self.emit_u32(loop_start as u32);

        let loop_end = self.bytecode.code.len();
        let loop_ctx = self.loop_stack.pop().unwrap();
        self.patch_u32(exit_jump, loop_end as u32);
        for jump_pos in loop_ctx.exit_jumps {
            self.patch_u32(jump_pos, loop_end as u32);
        }
        for jump_pos in loop_ctx.continue_jumps {
            self.patch_u32(jump_pos, continue_pos as u32);
        }

        self.emit_opcode(OP_JUMP);
        let skip_err = self.emit_u32(0);
        self.patch_u32(type_err, self.bytecode.code.len() as u32);
        self.emit_constant(Value::String(Arc::from(
            "for key, value in ... requires an object iterable",
        )));
        self.emit_opcode(OP_PRINT);
        self.emit_opcode(OP_NULL);
        self.patch_u32(skip_err, self.bytecode.code.len() as u32);

        Ok(())
    }

    fn try_compile_combined_while(
        &mut self,
        condition: &Expression,
        body: &[Statement],
    ) -> Result<Option<()>, String> {
        if let Expression::BinaryOp { left, op, right } = condition {
            if *op == BinaryOperator::LessThan {
                if let (Expression::Identifier(cond_var), Expression::Literal(Literal::Number(limit))) =
                    (&**left, &**right)
                {
                    if body.len() == 1 {
                        if let Statement::Assignment { name, value } = &body[0] {
                            if name == cond_var {
                                if let Expression::BinaryOp {
                                    left: body_left,
                                    op: body_op,
                                    right: body_right,
                                } = value
                                {
                                    if *body_op == BinaryOperator::Add {
                                        match (&**body_left, &**body_right) {
                                            (Expression::Identifier(src), Expression::Literal(Literal::Number(n)))
                                            | (Expression::Literal(Literal::Number(n)), Expression::Identifier(src))
                                                if src == cond_var =>
                                            {
                                                let idx = self.resolve_global(cond_var);
                                                if *n == 1 {
                                                    self.emit_opcode(OP_LOOP_INC_LESS);
                                                    self.emit_u32(idx as u32);
                                                    self.emit_i64(*limit);
                                                } else {
                                                    self.emit_opcode(OP_LOOP_STEP_LESS);
                                                    self.emit_u32(idx as u32);
                                                    self.emit_i64(*limit);
                                                    self.emit_i64(*n);
                                                }
                                                return Ok(Some(()));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }

                    self.emit_loop_hoists(body)?;
                    let idx = self.resolve_global(cond_var);
                    let loop_start = self.bytecode.code.len();
                    self.loop_stack.push(LoopContext {
                        start_pos: loop_start,
                        continue_jumps: Vec::new(),
                        exit_jumps: Vec::new(),
                    });
                    self.emit_opcode(OP_LESS_CONST_JUMP_IF_FALSE);
                    self.emit_u32(idx as u32);
                    self.emit_i64(*limit);
                    let exit_pos = self.emit_u32(0);
                    self.compile_block(body)?;
                    self.emit_opcode(OP_JUMP);
                    self.emit_u32(loop_start as u32);
                    let loop_end = self.bytecode.code.len();
                    let loop_ctx = self.loop_stack.pop().unwrap();
                    self.patch_u32(exit_pos, loop_end as u32);
                    for jump_pos in loop_ctx.exit_jumps {
                        self.patch_u32(jump_pos, loop_end as u32);
                    }
                    for jump_pos in loop_ctx.continue_jumps {
                        self.patch_u32(jump_pos, loop_start as u32);
                    }
                    self.member_hoist.clear();
                    return Ok(Some(()));
                }
            }
        }
        Ok(None)
    }

    fn compile_block(&mut self, block: &[Statement]) -> Result<(), String> {
        for statement in block {
            self.compile_statement(statement, false)?;
        }
        Ok(())
    }

    fn compile_special_assignment(&mut self, name: &str, value: &Expression) -> Result<Option<()>, String> {
        match value {
            Expression::BinaryOp { left, op, right } => {
                match (&**left, &**right) {
                    (Expression::Identifier(src), Expression::Literal(Literal::Number(n)))
                    | (Expression::Literal(Literal::Number(n)), Expression::Identifier(src))
                        if src == name =>
                    {
                        let index = self.resolve_global(name);
                        return match op {
                            BinaryOperator::Add => {
                                if *n == 1 {
                                    self.emit_opcode(OP_INC_GLOBAL);
                                    self.emit_u32(index as u32);
                                } else {
                                    self.emit_opcode(OP_ADD_GLOBAL);
                                    self.emit_u32(index as u32);
                                    self.emit_i64(*n);
                                }
                                Ok(Some(()))
                            }
                            BinaryOperator::Subtract => {
                                if let Expression::Identifier(_) = &**left {
                                    self.emit_opcode(OP_ADD_GLOBAL);
                                    self.emit_u32(index as u32);
                                    self.emit_i64(-*n);
                                    Ok(Some(()))
                                } else {
                                    Ok(None)
                                }
                            }
                            _ => Ok(None),
                        };
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    fn compile_expression(&mut self, expression: &Expression) -> Result<(), String> {
        match expression {
            Expression::Literal(literal) => {
                match literal {
                    Literal::Array { elements, mutable } => {
                        for elem in elements {
                            self.compile_expression(elem)?;
                        }
                        self.emit_opcode(OP_BUILD_ARRAY);
                        self.emit_u32(elements.len() as u32);
                        self.bytecode.emit_byte(if *mutable { 1 } else { 0 });
                    }
                    Literal::Object { fields } => {
                        for (_, elem) in fields {
                            self.compile_expression(elem)?;
                        }
                        self.emit_opcode(OP_BUILD_OBJECT);
                        self.emit_u32(fields.len() as u32);
                        for (key, _) in fields {
                            let key_idx = self.bytecode.add_constant(Value::String(Arc::from(
                                key.as_str(),
                            )));
                            self.emit_u32(key_idx);
                        }
                    }
                    _ => {
                        let val = match literal {
                            Literal::Number(n) => Value::Number(*n),
                            Literal::String(s) => Value::String(Arc::from(s.as_str())),
                            Literal::Boolean(b) => Value::Boolean(*b),
                            Literal::Null => Value::Null,
                            Literal::Array { .. } | Literal::Object { .. } => unreachable!(),
                        };
                        self.emit_constant(val);
                    }
                }
            }
            Expression::Identifier(name) => {
                let resolved = self.resolve_identifier(name)?;
                self.emit_load_var(resolved)?;
            }
            Expression::Input { types, prompt } => {
                self.compile_expression(prompt)?;

                let mut type_mask: u8 = 0;
                for input_type in types {
                    match input_type {
                        crate::ast::InputType::String => type_mask |= 0x01,
                        crate::ast::InputType::Int => type_mask |= 0x02,
                        crate::ast::InputType::Float => type_mask |= 0x04,
                    }
                }

                self.emit_opcode(OP_INPUT);
                self.bytecode.emit_byte(type_mask);
            }
            Expression::UnaryOp { op, expr } => {
                self.compile_expression(expr)?;
                match op {
                    UnaryOperator::Not => self.emit_opcode(OP_NOT),
                };
            }
            Expression::BinaryOp { left, op, right } => {
                if let Some(val) = fold_binary_literal(left, *op, right) {
                    self.emit_constant(val);
                    return Ok(());
                }
                match op {
                    BinaryOperator::And => {
                        self.compile_expression(left)?;
                        self.emit_opcode(OP_JUMP_IF_FALSE);
                        let end = self.emit_u32(0);
                        self.emit_opcode(OP_POP);
                        self.compile_expression(right)?;
                        self.patch_u32(end, self.bytecode.code.len() as u32);
                    }
                    BinaryOperator::Or => {
                        self.compile_expression(left)?;
                        self.emit_opcode(OP_JUMP_IF_FALSE);
                        let try_right = self.emit_u32(0);
                        self.emit_opcode(OP_JUMP);
                        let end = self.emit_u32(0);
                        self.patch_u32(try_right, self.bytecode.code.len() as u32);
                        self.emit_opcode(OP_POP);
                        self.compile_expression(right)?;
                        self.patch_u32(end, self.bytecode.code.len() as u32);
                    }
                    _ => {
                        self.compile_expression(left)?;
                        self.compile_expression(right)?;
                        let opcode = match op {
                            BinaryOperator::Add => OP_ADD,
                            BinaryOperator::Subtract => OP_SUBTRACT,
                            BinaryOperator::Multiply => OP_MULTIPLY,
                            BinaryOperator::Divide => OP_DIVIDE,
                            BinaryOperator::Equals => OP_EQUALS,
                            BinaryOperator::NotEquals => OP_NOT_EQUALS,
                            BinaryOperator::GreaterThan => OP_GREATER,
                            BinaryOperator::LessThan => OP_LESS,
                            BinaryOperator::GreaterOrEqual => OP_GREATER_EQUAL,
                            BinaryOperator::LessOrEqual => OP_LESS_EQUAL,
                            BinaryOperator::And | BinaryOperator::Or => unreachable!(),
                        };
                        self.emit_opcode(opcode);
                    }
                }
            }
            Expression::FunctionCall { name, args } => {
                if self.try_compile_builtin_call(name, args)? {
                    return Ok(());
                }

                for arg in args {
                    self.compile_expression(arg)?;
                }

                let resolved = self.resolve_identifier(name)?;
                self.emit_load_var(resolved)?;

                self.emit_opcode(OP_CALL_FUNCTION);
                self.emit_u32(args.len() as u32);
            }
            Expression::Index { object, index } => {
                self.compile_expression(object)?;
                self.compile_expression(index)?;
                self.emit_opcode(OP_INDEX_GET);
            }
            Expression::MemberAccess { object, member } => {
                if let Expression::Identifier(obj) = object.as_ref() {
                    let hoisted = self
                        .member_hoist
                        .get(&(obj.clone(), member.clone()))
                        .cloned();
                    if let Some(temp) = hoisted {
                        let idx = self.resolve_global(&temp);
                        self.emit_opcode(OP_LOAD_GLOBAL);
                        self.emit_u32(idx as u32);
                        return Ok(());
                    }
                }
                if let Expression::Identifier(alias) = object.as_ref() {
                    if let Some(exports) = self.import_bindings.get(alias) {
                        if let Some(&global_idx) = exports.get(member) {
                            self.emit_opcode(OP_LOAD_GLOBAL);
                            self.emit_u32(global_idx as u32);
                            return Ok(());
                        }
                        return Err(format!(
                            "Module '{}' has no exported function '{}'",
                            alias, member
                        ));
                    }
                }
                self.compile_expression(object)?;
                if member == "length" {
                    self.emit_opcode(OP_MEMBER_LENGTH);
                } else {
                    let key_idx =
                        self.bytecode.add_constant(Value::String(Arc::from(member.as_str())));
                    self.emit_opcode(OP_OBJECT_GET_CONST);
                    self.emit_u32(key_idx);
                }
            }
            Expression::Call { callee, args } => {
                if let Expression::Identifier(name) = callee.as_ref() {
                    if self.try_compile_builtin_call(name, args)? {
                        return Ok(());
                    }
                }

                for arg in args {
                    self.compile_expression(arg)?;
                }
                self.compile_expression(callee)?;
                self.emit_opcode(OP_CALL_FUNCTION);
                self.emit_u32(args.len() as u32);
            }
            Expression::FunctionExpr { params, body } => {
                let (func_offset, num_params, captures) = self.compile_function(params, body)?;
                self.emit_make_closure(func_offset, num_params, &captures);
            }
        }
        Ok(())
    }

    /// Core builtins (`push`, `pop`, `rmv`, `type`, `wait`). Returns true if handled.
    fn try_compile_builtin_call(
        &mut self,
        name: &str,
        args: &[Expression],
    ) -> Result<bool, String> {
        match name {
            "push" => {
                if args.len() != 2 {
                    return Err("push() expects 2 arguments".to_string());
                }
                self.compile_expression(&args[0])?;
                self.compile_expression(&args[1])?;
                self.emit_opcode(OP_ARRAY_PUSH);
                self.emit_opcode(OP_NULL);
            }
            "pop" => {
                if args.len() != 1 {
                    return Err("pop() expects 1 argument".to_string());
                }
                self.compile_expression(&args[0])?;
                self.emit_opcode(OP_ARRAY_POP);
            }
            "rmv" => {
                if args.len() != 1 {
                    return Err("rmv() expects 1 argument".to_string());
                }
                self.compile_path_delete(&args[0])?;
                self.emit_opcode(OP_NULL);
            }
            "type" => {
                if args.len() != 1 {
                    return Err("type() expects 1 argument".to_string());
                }
                self.compile_expression(&args[0])?;
                self.emit_opcode(OP_TYPE);
            }
            "wait" => {
                if args.len() != 1 {
                    return Err("wait() expects 1 argument".to_string());
                }
                self.compile_expression(&args[0])?;
                self.emit_opcode(OP_WAIT);
                self.emit_opcode(OP_NULL);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn register_upvalue(&mut self, name: &str, kind: u8, index: u32) {
        let scope = self.scope_stack.last_mut().unwrap();
        scope.upvalue_names.push(name.to_string());
        scope.captures.push(UpvalueCapture { kind, index });
    }

    fn resolve_identifier(&mut self, name: &str) -> Result<VarResolution, String> {
        if self.scope_stack.is_empty() {
            return Ok(VarResolution::Global(self.resolve_global(name)));
        }

        let current = self.scope_stack.len() - 1;

        if let Some(&index) = self.scope_stack[current].locals.get(name) {
            return Ok(VarResolution::Local(index));
        }

        if let Some(index) = self
            .scope_stack[current]
            .upvalue_names
            .iter()
            .position(|n| n == name)
        {
            return Ok(VarResolution::Upvalue(index));
        }

        for up in (0..current).rev() {
            if let Some(&local_idx) = self.scope_stack[up].locals.get(name) {
                self.register_upvalue(name, CAPTURE_LOCAL, local_idx as u32);
                let index = self.scope_stack[current].upvalue_names.len() - 1;
                return Ok(VarResolution::Upvalue(index));
            }
            if let Some(uv_idx) = self
                .scope_stack[up]
                .upvalue_names
                .iter()
                .position(|n| n == name)
            {
                self.register_upvalue(name, CAPTURE_UPVALUE, uv_idx as u32);
                let index = self.scope_stack[current].upvalue_names.len() - 1;
                return Ok(VarResolution::Upvalue(index));
            }
        }

        Ok(VarResolution::Global(self.resolve_global(name)))
    }

    fn resolve_global(&mut self, name: &str) -> usize {
        if let Some(&index) = self.globals.get(name) {
            index
        } else {
            let index = self.globals.len();
            self.globals.insert(name.to_string(), index);
            index
        }
    }

    fn flatten_path(expr: &Expression) -> Result<(Expression, Vec<PathSegment>), String> {
        match expr {
            Expression::MemberAccess { object, member } => {
                let (root, mut segs) = Self::flatten_path(object)?;
                segs.push(PathSegment::Dot(member.clone()));
                Ok((root, segs))
            }
            Expression::Index { object, index } => {
                let (root, mut segs) = Self::flatten_path(object)?;
                segs.push(PathSegment::Bracket(index.clone()));
                Ok((root, segs))
            }
            other => Ok((other.clone(), Vec::new())),
        }
    }

    fn compile_property_assign(
        &mut self,
        target: &Expression,
        value: &Expression,
    ) -> Result<(), String> {
        let (root, segments) = Self::flatten_path(target)?;
        if segments.is_empty() {
            return Err("Invalid property assignment target".to_string());
        }

        let path_id = self.path_counter;
        self.path_counter += 1;
        let cur_name = format!("__path_cur_{}", path_id);

        self.compile_expression(&root)?;
        let cur_idx = self.resolve_global(&cur_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(cur_idx as u32);

        let last = segments.len() - 1;
        for seg in &segments[..last] {
            self.emit_opcode(OP_LOAD_GLOBAL);
            self.emit_u32(cur_idx as u32);
            match seg {
                PathSegment::Dot(name) => {
                    let key_idx =
                        self.bytecode.add_constant(Value::String(Arc::from(name.as_str())));
                    self.emit_opcode(OP_OBJECT_GET_OR_CREATE_CONST);
                    self.emit_u32(key_idx);
                }
                PathSegment::Bracket(index_expr) => {
                    self.compile_expression(index_expr)?;
                    self.emit_opcode(OP_OBJECT_GET_OR_CREATE);
                }
            }
            self.emit_opcode(OP_STORE_GLOBAL);
            self.emit_u32(cur_idx as u32);
        }

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(cur_idx as u32);
        match &segments[last] {
            PathSegment::Dot(name) => {
                let key_idx =
                    self.bytecode.add_constant(Value::String(Arc::from(name.as_str())));
                self.emit_opcode(OP_CONSTANT);
                self.emit_u32(key_idx);
                self.compile_expression(value)?;
                self.emit_opcode(OP_OBJECT_SET);
            }
            PathSegment::Bracket(index_expr) => {
                self.compile_expression(index_expr)?;
                self.compile_expression(value)?;
                self.emit_opcode(OP_OBJECT_SET);
            }
        }

        Ok(())
    }

    fn compile_path_delete(&mut self, target: &Expression) -> Result<(), String> {
        let (root, segments) = Self::flatten_path(target)?;
        if segments.is_empty() {
            return Err("rmv() requires a property access target".to_string());
        }

        let path_id = self.path_counter;
        self.path_counter += 1;
        let cur_name = format!("__path_cur_{}", path_id);

        self.compile_expression(&root)?;
        let cur_idx = self.resolve_global(&cur_name);
        self.emit_opcode(OP_STORE_GLOBAL);
        self.emit_u32(cur_idx as u32);

        let last = segments.len() - 1;
        for seg in &segments[..last] {
            self.emit_opcode(OP_LOAD_GLOBAL);
            self.emit_u32(cur_idx as u32);
            match seg {
                PathSegment::Dot(name) => {
                    let key_idx =
                        self.bytecode.add_constant(Value::String(Arc::from(name.as_str())));
                    self.emit_opcode(OP_OBJECT_GET_CONST);
                    self.emit_u32(key_idx);
                }
                PathSegment::Bracket(index_expr) => {
                    self.compile_expression(index_expr)?;
                    self.emit_opcode(OP_OBJECT_GET);
                }
            }
            self.emit_opcode(OP_STORE_GLOBAL);
            self.emit_u32(cur_idx as u32);
        }

        self.emit_opcode(OP_LOAD_GLOBAL);
        self.emit_u32(cur_idx as u32);
        match &segments[last] {
            PathSegment::Dot(name) => {
                let key_idx =
                    self.bytecode.add_constant(Value::String(Arc::from(name.as_str())));
                self.emit_opcode(OP_CONSTANT);
                self.emit_u32(key_idx);
                self.emit_opcode(OP_OBJECT_DELETE);
            }
            PathSegment::Bracket(index_expr) => {
                self.compile_expression(index_expr)?;
                self.emit_opcode(OP_OBJECT_DELETE);
            }
        }

        Ok(())
    }
}

enum PathSegment {
    Dot(String),
    Bracket(Box<Expression>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn type_builtin_emits_opcode() {
        let tokens = Lexer::new("print(type(42))").tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = compiler.finish_bytecode();
        assert!(
            bc.code.contains(&OP_TYPE),
            "expected OP_TYPE in {:?}",
            bc.code
        );
    }

    #[test]
    fn type_builtin_runs() {
        use crate::optimizer::optimize;
        use crate::vm::VM;

        let tokens = Lexer::new("print(type(42))").tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = optimize(compiler.finish_bytecode());
        let mut vm = VM::new();
        vm.execute(&bc).expect("run type()");
    }

    #[test]
    fn type_builtin_via_call_expression() {
        use crate::optimizer::optimize;
        use crate::vm::VM;

        let tokens = Lexer::new("print((type)(42))").tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = optimize(compiler.finish_bytecode());
        let mut vm = VM::new();
        vm.execute(&bc).expect("run parenthesized type()");
    }

    #[test]
    fn if_in_function_with_optimize() {
        use crate::optimizer::optimize;
        use crate::vm::VM;

        let src = "fn test(n) {\n    if (true) {\n        return n\n    }\n    return 0\n}\nprint(test(5))";
        let tokens = Lexer::new(src).tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = optimize(compiler.finish_bytecode());
        let mut vm = VM::new();
        vm.execute(&bc).expect("run if in function");
    }

    #[test]
    fn member_hoist_in_loop() {
        use crate::optimizer::optimize;
        use crate::vm::VM;

        let src = "set arr = [1, 2, 3]\nset sum = 0\nset i = 0\nwhile (i < 2) {\n    set sum = sum + arr.length\n    set i = i + 1\n}\nprint(sum)";
        let tokens = Lexer::new(src).tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = optimize(compiler.finish_bytecode());
        let mut vm = VM::new();
        vm.execute(&bc).expect("run member hoist");
    }

    #[test]
    fn wait_builtin_runs() {
        use crate::optimizer::optimize;
        use crate::vm::VM;

        let tokens = Lexer::new("wait(0)").tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&stmts).expect("compile");
        let bc = optimize(compiler.finish_bytecode());
        let mut vm = VM::new();
        vm.execute(&bc).expect("run wait()");
    }
}
