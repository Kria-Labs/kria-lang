use crate::ast::{BinaryOperator, Expression, Literal, Statement, UnaryOperator};
use crate::vm::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    Local(u32),
    Upvalue(u32),
}

#[derive(Debug, Clone)]
pub enum SsaInstruction {
    Const {
        dst: ValueId,
        idx: u32,
    },
    Null {
        dst: ValueId,
    },
    LoadGlobal {
        dst: ValueId,
        slot: u32,
    },
    StoreGlobal {
        slot: u32,
        value: ValueId,
    },
    LoadLocal {
        dst: ValueId,
        slot: u32,
    },
    StoreLocal {
        slot: u32,
        value: ValueId,
    },
    LoadUpvalue {
        dst: ValueId,
        slot: u32,
    },
    StoreUpvalue {
        slot: u32,
        value: ValueId,
    },
    Move {
        dst: ValueId,
        src: ValueId,
    },
    Unary {
        dst: ValueId,
        op: UnaryOperator,
        value: ValueId,
    },
    Binary {
        dst: ValueId,
        op: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    BinaryInt {
        dst: ValueId,
        op: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    Print {
        value: ValueId,
    },
    Input {
        dst: ValueId,
        prompt: ValueId,
        type_mask: u8,
    },
    Call {
        dst: ValueId,
        callee: ValueId,
        args: Vec<ValueId>,
    },
    CallDirect {
        dst: ValueId,
        function_index: u32,
        args: Vec<ValueId>,
    },
    MakeClosure {
        dst: ValueId,
        function: u32,
        captures: Vec<CaptureKind>,
    },
    Return {
        value: Option<ValueId>,
    },
    BuildArray {
        dst: ValueId,
        elements: Vec<ValueId>,
        mutable: bool,
    },
    BuildObject {
        dst: ValueId,
        fields: Vec<(String, ValueId)>,
    },
    IndexGet {
        dst: ValueId,
        object: ValueId,
        index: ValueId,
    },
    IndexSet {
        object: ValueId,
        index: ValueId,
        value: ValueId,
    },
    PropertyGet {
        dst: ValueId,
        object: ValueId,
        member: String,
    },
    PropertySet {
        object: ValueId,
        member: String,
        value: ValueId,
    },
    ArrayLen {
        dst: ValueId,
        object: ValueId,
    },
    ArrayPush {
        object: ValueId,
        value: ValueId,
    },
    ArrayPop {
        dst: ValueId,
        object: ValueId,
    },
    ObjectKeys {
        dst: ValueId,
        object: ValueId,
    },
    ObjectDelete {
        object: ValueId,
        member: String,
    },
    ObjectDeleteKey {
        object: ValueId,
        key: ValueId,
    },
    IsArray {
        dst: ValueId,
        value: ValueId,
    },
    IsObject {
        dst: ValueId,
        value: ValueId,
    },
    MemberLength {
        dst: ValueId,
        object: ValueId,
    },
    Type {
        dst: ValueId,
        value: ValueId,
    },
    Wait {
        dst: ValueId,
        value: ValueId,
    },
    AddGlobal {
        slot: u32,
        delta: i64,
    },
    AddLocal {
        slot: u32,
        delta: i64,
    },
    AppendStringGlobal {
        slot: u32,
        suffix: String,
    },
    LoopIncLess {
        slot: u32,
        limit: i64,
        step: i64,
        local: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SsaPhi {
    pub dst: ValueId,
    pub incoming: Vec<(ValueId, BlockId)>,
}

#[derive(Debug, Clone)]
pub enum SsaTerminator {
    Fallthrough,
    Jump(BlockId),
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    CompareLessConst {
        slot: u32,
        limit: i64,
        local: bool,
        then_block: BlockId,
        else_block: BlockId,
    },
    TailCall {
        function_index: u32,
        args: Vec<ValueId>,
    },
    Return(Option<ValueId>),
}

#[derive(Debug, Clone)]
pub struct SsaBlock {
    pub id: BlockId,
    pub predecessors: Vec<BlockId>,
    pub phis: Vec<SsaPhi>,
    pub instructions: Vec<SsaInstruction>,
    pub terminator: SsaTerminator,
}

impl SsaBlock {
    fn new(id: BlockId) -> Self {
        Self {
            id,
            predecessors: Vec::new(),
            phis: Vec::new(),
            instructions: Vec::new(),
            terminator: SsaTerminator::Fallthrough,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SsaFunction {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub captures: Vec<CaptureKind>,
    pub blocks: Vec<SsaBlock>,
    pub max_registers: u32,
    pub local_count: u32,
    pub constants: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct SsaModule {
    pub constants: Vec<Value>,
    pub globals: HashMap<String, u32>,
    pub functions: Vec<SsaFunction>,
    pub entry: SsaFunction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Variable {
    Global(u32),
    Local(u32),
    Upvalue(u32),
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    locals: HashMap<String, u32>,
    next_local: u32,
    upvalue_names: Vec<String>,
    captures: Vec<CaptureKind>,
    is_module: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleContext {
    pub globals: HashMap<String, u32>,
    pub module_functions: Vec<SsaFunction>,
    pub imports: HashMap<String, HashMap<String, u32>>,
    pub next_function: u32,
    pub function_globals: HashMap<String, u32>,
    pub inline_functions: HashMap<String, (Vec<String>, Vec<Statement>)>,
    /// Redefined or reassigned functions: never inlined or bound statically.
    pub inline_blocked: HashSet<String>,
    pub module_prefix: String,
}

impl ModuleContext {
    /// `::` can't appear in identifiers, so prefixed keys never clash with user names.
    pub fn qualify(&self, name: &str) -> String {
        if self.module_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}{}", self.module_prefix, name)
        }
    }

    pub fn resolve_global(&mut self, name: &str) -> u32 {
        let key = self.qualify(name);
        if let Some(slot) = self.globals.get(&key) {
            return *slot;
        }
        let slot = self.globals.len() as u32;
        self.globals.insert(key, slot);
        slot
    }

    pub fn lookup_global(&self, name: &str) -> Option<u32> {
        self.globals.get(&self.qualify(name)).copied()
    }
}

#[derive(Debug, Clone)]
pub struct ModuleUnit {
    pub name: String,
    pub imports: Vec<(String, usize)>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct ModuleLowerer {
    pub context: std::rc::Rc<std::cell::RefCell<ModuleContext>>,
    scope_stack: std::rc::Rc<std::cell::RefCell<Vec<ScopeFrame>>>,
}

impl ScopeFrame {
    fn allocate_local(&mut self) -> u32 {
        let slot = self.next_local;
        self.next_local += 1;
        slot
    }
}

fn optimize_ssa_function(function: &mut SsaFunction) {
    for block in &mut function.blocks {
        let mut copies = HashMap::<ValueId, ValueId>::new();
        let mut optimized = Vec::with_capacity(block.instructions.len());
        for mut instruction in block.instructions.drain(..) {
            match &mut instruction {
                SsaInstruction::Move { dst, src } => {
                    if dst == src {
                        continue;
                    }
                    copies.insert(*dst, *src);
                }
                SsaInstruction::Binary { left, right, .. } => {
                    if let Some(source) = copies.get(left) {
                        *left = *source;
                    }
                    if let Some(source) = copies.get(right) {
                        *right = *source;
                    }
                }
                SsaInstruction::BinaryInt { left, right, .. } => {
                    if let Some(source) = copies.get(left) {
                        *left = *source;
                    }
                    if let Some(source) = copies.get(right) {
                        *right = *source;
                    }
                }
                SsaInstruction::Call { callee, args, .. } => {
                    if let Some(source) = copies.get(callee) {
                        *callee = *source;
                    }
                    for arg in args {
                        if let Some(source) = copies.get(arg) {
                            *arg = *source;
                        }
                    }
                }
                SsaInstruction::CallDirect { args, .. } => {
                    for arg in args {
                        if let Some(source) = copies.get(arg) {
                            *arg = *source;
                        }
                    }
                }
                _ => {}
            }
            optimized.push(instruction);
        }
        block.instructions = optimized;
        block.instructions.retain(|instruction| {
            !matches!(
                instruction,
                SsaInstruction::Move { dst, src } if dst == src
            )
        });
    }
}

fn collect_function_bindings(
    statements: &[Statement],
    definitions: &mut HashMap<String, usize>,
    rebound: &mut HashSet<String>,
) {
    for statement in statements {
        match statement {
            Statement::FunctionDef { name, body, .. } => {
                *definitions.entry(name.clone()).or_insert(0) += 1;
                collect_function_bindings(body, definitions, rebound);
            }
            Statement::Assignment { name, .. } => {
                rebound.insert(name.clone());
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (_, body) in branches {
                    collect_function_bindings(body, definitions, rebound);
                }
                if let Some(body) = else_branch {
                    collect_function_bindings(body, definitions, rebound);
                }
            }
            Statement::While { body, .. } => {
                collect_function_bindings(body, definitions, rebound);
            }
            Statement::ForIn {
                key_name,
                value_name,
                body,
                ..
            } => {
                rebound.insert(key_name.clone());
                if let Some(value_name) = value_name {
                    rebound.insert(value_name.clone());
                }
                collect_function_bindings(body, definitions, rebound);
            }
            _ => {}
        }
    }
}

/// Unlike `Value::eq`, never treats `7` and `7.0` as the same constant.
fn constants_identical(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct FunctionLowerer {
    module: ModuleLowerer,
    current: BlockId,
    next_value: u32,
    next_block: u32,
    blocks: Vec<SsaBlock>,
    loop_exits: Vec<(BlockId, BlockId)>,
    constants: Vec<Value>,
    params: Vec<String>,
    is_module: bool,
}

impl ModuleLowerer {
    pub fn new() -> Self {
        Self {
            context: std::rc::Rc::new(std::cell::RefCell::new(ModuleContext::default())),
            scope_stack: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }

    pub fn bind_import(&mut self, alias: &str, exports: HashMap<String, u32>) {
        let mut ctx = self.context.borrow_mut();
        let key = ctx.qualify(alias);
        ctx.imports.insert(key, exports);
    }

    pub fn resolve_global(&mut self, name: &str) -> u32 {
        self.context.borrow_mut().resolve_global(name)
    }

    pub fn lower_module(&mut self, statements: &[Statement]) -> Result<SsaModule, String> {
        self.lower_units(&[], statements)
    }

    pub fn lower_program(&mut self, units: &[ModuleUnit]) -> Result<SsaModule, String> {
        let Some((entry, dependencies)) = units.split_last() else {
            return self.lower_module(&[]);
        };
        if entry
            .imports
            .iter()
            .any(|(_, dep)| *dep >= dependencies.len())
        {
            return Err("Internal error: import refers to a later module".to_string());
        }
        self.lower_units(units, &entry.statements)
    }

    fn register_function_bindings(&mut self, statements: &[Statement]) {
        let mut definitions = HashMap::new();
        let mut rebound = HashSet::new();
        collect_function_bindings(statements, &mut definitions, &mut rebound);
        let mut ctx = self.context.borrow_mut();
        for (name, count) in definitions {
            if count > 1 {
                let key = ctx.qualify(&name);
                ctx.inline_blocked.insert(key);
            }
        }
        for name in rebound {
            let key = ctx.qualify(&name);
            ctx.inline_blocked.insert(key);
        }
    }

    fn enter_unit(
        &mut self,
        unit: &ModuleUnit,
        prefix: String,
        exports_by_unit: &[HashMap<String, u32>],
    ) -> Result<HashMap<String, u32>, String> {
        self.context.borrow_mut().module_prefix = prefix;
        for (alias, dep) in &unit.imports {
            let exports = exports_by_unit.get(*dep).ok_or_else(|| {
                format!(
                    "Internal error: module '{}' imported before it was compiled",
                    alias
                )
            })?;
            self.bind_import(alias, exports.clone());
        }
        let mut exports = HashMap::new();
        for statement in &unit.statements {
            if let Statement::FunctionDef {
                name,
                exported: true,
                ..
            } = statement
            {
                exports.insert(name.clone(), self.resolve_global(name));
            }
        }
        self.register_function_bindings(&unit.statements);
        Ok(exports)
    }

    fn lower_units(
        &mut self,
        units: &[ModuleUnit],
        single: &[Statement],
    ) -> Result<SsaModule, String> {
        self.scope_stack.borrow_mut().push(ScopeFrame {
            locals: HashMap::new(),
            next_local: 0,
            upvalue_names: Vec::new(),
            captures: Vec::new(),
            is_module: true,
        });
        let mut function = FunctionLowerer::new_module(self.clone());
        let lowered = if units.is_empty() {
            self.register_function_bindings(single);
            function.lower_statements(single)
        } else {
            let mut exports_by_unit: Vec<HashMap<String, u32>> = Vec::new();
            let last = units.len() - 1;
            let mut result = Ok(());
            for (index, unit) in units.iter().enumerate() {
                let prefix = if index == last {
                    String::new()
                } else {
                    format!("{}:{}::", index, unit.name)
                };
                let exports = match self.enter_unit(unit, prefix, &exports_by_unit) {
                    Ok(exports) => exports,
                    Err(error) => {
                        result = Err(error);
                        break;
                    }
                };
                exports_by_unit.push(exports);
                if let Err(error) = function.lower_statements(&unit.statements) {
                    result = Err(error);
                    break;
                }
            }
            self.context.borrow_mut().module_prefix = String::new();
            result
        };
        if let Err(error) = lowered {
            self.scope_stack.borrow_mut().pop();
            return Err(error);
        }
        let mut entry = function.finish_function();
        self.scope_stack.borrow_mut().pop();

        let mut ctx = self.context.borrow().clone();
        optimize_ssa_function(&mut entry);
        for function in &mut ctx.module_functions {
            optimize_ssa_function(function);
        }
        Ok(SsaModule {
            constants: entry.constants.clone(),
            globals: ctx.globals,
            functions: ctx.module_functions,
            entry,
        })
    }
}

impl FunctionLowerer {
    fn new_module(module: ModuleLowerer) -> Self {
        let entry = BlockId(0);
        Self {
            module,
            current: entry,
            next_value: 0,
            next_block: 1,
            blocks: vec![SsaBlock::new(entry)],
            loop_exits: Vec::new(),
            constants: Vec::new(),
            params: Vec::new(),
            is_module: true,
        }
    }

    fn new_block(&mut self, predecessor: Option<BlockId>) -> BlockId {
        let block = BlockId(self.next_block);
        self.next_block += 1;
        let mut ssa_block = SsaBlock::new(block);
        if let Some(pred) = predecessor {
            ssa_block.predecessors.push(pred);
        }
        self.blocks.push(ssa_block);
        block
    }

    fn block_mut(&mut self, id: BlockId) -> &mut SsaBlock {
        &mut self.blocks[id.0 as usize]
    }

    fn current_mut(&mut self) -> &mut SsaBlock {
        &mut self.blocks[self.current.0 as usize]
    }

    fn emit(&mut self, instruction: SsaInstruction) -> ValueId {
        let dst = match instruction {
            SsaInstruction::Const { dst, .. }
            | SsaInstruction::Null { dst }
            | SsaInstruction::LoadGlobal { dst, .. }
            | SsaInstruction::LoadLocal { dst, .. }
            | SsaInstruction::LoadUpvalue { dst, .. }
            | SsaInstruction::Move { dst, .. }
            | SsaInstruction::Unary { dst, .. }
            | SsaInstruction::Binary { dst, .. }
            | SsaInstruction::BinaryInt { dst, .. }
            | SsaInstruction::Input { dst, .. }
            | SsaInstruction::Call { dst, .. }
            | SsaInstruction::CallDirect { dst, .. }
            | SsaInstruction::MakeClosure { dst, .. }
            | SsaInstruction::BuildArray { dst, .. }
            | SsaInstruction::BuildObject { dst, .. }
            | SsaInstruction::IndexGet { dst, .. }
            | SsaInstruction::PropertyGet { dst, .. }
            | SsaInstruction::ArrayLen { dst, .. }
            | SsaInstruction::ArrayPop { dst, .. }
            | SsaInstruction::ObjectKeys { dst, .. }
            | SsaInstruction::IsArray { dst, .. }
            | SsaInstruction::IsObject { dst, .. }
            | SsaInstruction::MemberLength { dst, .. }
            | SsaInstruction::Type { dst, .. }
            | SsaInstruction::Wait { dst, .. } => dst,
            _ => ValueId(u32::MAX),
        };
        self.current_mut().instructions.push(instruction);
        dst
    }

    fn fresh_value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn resolve_variable(&mut self, name: &str) -> Variable {
        let mut stack = self.module.scope_stack.borrow_mut();
        if stack.is_empty() {
            return Variable::Global(self.module.context.borrow_mut().resolve_global(name));
        }
        let current = stack.len() - 1;
        if let Some(&slot) = stack[current].locals.get(name) {
            return Variable::Local(slot);
        }
        if stack[current].is_module {
            return Variable::Global(self.module.context.borrow_mut().resolve_global(name));
        }
        if let Some(pos) = stack[current].upvalue_names.iter().position(|n| n == name) {
            return Variable::Upvalue(pos as u32);
        }
        for up in (0..current).rev() {
            if stack[up].is_module {
                break;
            }
            if let Some(&local_slot) = stack[up].locals.get(name) {
                let mut prev_kind = CaptureKind::Local(local_slot);
                for target_idx in (up + 1)..=current {
                    if let Some(pos) = stack[target_idx]
                        .upvalue_names
                        .iter()
                        .position(|n| n == name)
                    {
                        prev_kind = CaptureKind::Upvalue(pos as u32);
                    } else {
                        let new_pos = stack[target_idx].upvalue_names.len() as u32;
                        stack[target_idx].upvalue_names.push(name.to_string());
                        stack[target_idx].captures.push(prev_kind);
                        prev_kind = CaptureKind::Upvalue(new_pos);
                    }
                }
                let final_pos = stack[current].upvalue_names.len() - 1;
                return Variable::Upvalue(final_pos as u32);
            }
            if let Some(pos) = stack[up].upvalue_names.iter().position(|n| n == name) {
                let mut prev_kind = CaptureKind::Upvalue(pos as u32);
                for target_idx in (up + 1)..=current {
                    if let Some(pos) = stack[target_idx]
                        .upvalue_names
                        .iter()
                        .position(|n| n == name)
                    {
                        prev_kind = CaptureKind::Upvalue(pos as u32);
                    } else {
                        let new_pos = stack[target_idx].upvalue_names.len() as u32;
                        stack[target_idx].upvalue_names.push(name.to_string());
                        stack[target_idx].captures.push(prev_kind);
                        prev_kind = CaptureKind::Upvalue(new_pos);
                    }
                }
                let final_pos = stack[current].upvalue_names.len() - 1;
                return Variable::Upvalue(final_pos as u32);
            }
        }
        Variable::Global(self.module.context.borrow_mut().resolve_global(name))
    }

    fn add_constant(&mut self, value: Value) -> u32 {
        let index = if let Some(index) = self
            .constants
            .iter()
            .position(|item| constants_identical(item, &value))
        {
            index
        } else {
            let index = self.constants.len();
            self.constants.push(value);
            index
        };
        index as u32
    }

    fn lower_statements(&mut self, statements: &[Statement]) -> Result<(), String> {
        for statement in statements {
            self.lower_statement(statement)?;
            if !matches!(self.current_mut().terminator, SsaTerminator::Fallthrough) {
                break;
            }
        }
        Ok(())
    }

    fn lower_statement(&mut self, statement: &Statement) -> Result<(), String> {
        match statement {
            Statement::Assignment { name, value } => {
                {
                    let mut ctx = self.module.context.borrow_mut();
                    let key = ctx.qualify(name);
                    ctx.inline_blocked.insert(key);
                }
                if self.try_special_assignment(name, value)? {
                    return Ok(());
                }
                let value = self.lower_expression(value)?;
                let var = if !self.is_module {
                    let exists = {
                        let stack = self.module.scope_stack.borrow();
                        let current = stack.len() - 1;
                        stack[current].locals.contains_key(name)
                    };
                    if !exists {
                        let mut stack = self.module.scope_stack.borrow_mut();
                        let current = stack.len() - 1;
                        let slot = stack[current].allocate_local();
                        stack[current].locals.insert(name.clone(), slot);
                        Variable::Local(slot)
                    } else {
                        self.resolve_variable(name)
                    }
                } else {
                    self.resolve_variable(name)
                };

                match var {
                    Variable::Global(slot) => {
                        self.emit(SsaInstruction::StoreGlobal { slot, value });
                    }
                    Variable::Local(slot) => {
                        self.emit(SsaInstruction::StoreLocal { slot, value });
                    }
                    Variable::Upvalue(slot) => {
                        self.emit(SsaInstruction::StoreUpvalue { slot, value });
                    }
                }
            }
            Statement::Print(expression) => {
                let value = self.lower_expression(expression)?;
                self.emit(SsaInstruction::Print { value });
            }
            Statement::Expression(expression) => {
                let _ = self.lower_expression(expression)?;
            }
            Statement::Return(expression) => {
                if let Some(Expression::Call { callee, args }) = expression.as_ref() {
                    if let Expression::Identifier(name) = callee.as_ref() {
                        let function_index =
                            if matches!(self.lookup_variable(name), Some(Variable::Global(_))) {
                                self.static_function_index(name)
                            } else {
                                None
                            };
                        if let Some(function_index) = function_index {
                            let mut values = Vec::with_capacity(args.len());
                            for arg in args {
                                values.push(self.lower_expression(arg)?);
                            }
                            self.current_mut().terminator = SsaTerminator::TailCall {
                                function_index,
                                args: values,
                            };
                            return Ok(());
                        }
                    }
                }
                let value = match expression {
                    Some(expression) => Some(self.lower_expression(expression)?),
                    None => None,
                };
                self.emit(SsaInstruction::Return { value });
                self.current_mut().terminator = SsaTerminator::Return(value);
            }
            Statement::If {
                branches,
                else_branch,
            } => self.lower_if(branches, else_branch)?,
            Statement::While { condition, body } => self.lower_while(condition, body)?,
            Statement::IndexAssign {
                object,
                index,
                value,
            } => {
                let object = self.lower_expression(object)?;
                let index = self.lower_expression(index)?;
                let value = self.lower_expression(value)?;
                self.emit(SsaInstruction::IndexSet {
                    object,
                    index,
                    value,
                });
            }
            Statement::PropertyAssign { target, value } => {
                let value = self.lower_expression(value)?;
                if let Expression::MemberAccess { object, member } = target.as_ref() {
                    let object = self.lower_expression(object)?;
                    self.emit(SsaInstruction::PropertySet {
                        object,
                        member: member.clone(),
                        value,
                    });
                } else {
                    return Err(
                        "Register lowering only supports named property assignment".to_string()
                    );
                }
            }
            Statement::ForIn {
                key_name,
                value_name,
                iterable,
                body,
            } => self.lower_for_in(key_name, value_name.as_deref(), iterable, body)?,
            Statement::Break => {
                if let Some((exit, _)) = self.loop_exits.last() {
                    self.current_mut().terminator = SsaTerminator::Jump(*exit);
                } else {
                    return Err("break can only be used inside a loop".to_string());
                }
            }
            Statement::Continue => {
                if let Some((_, continue_block)) = self.loop_exits.last() {
                    self.current_mut().terminator = SsaTerminator::Jump(*continue_block);
                } else {
                    return Err("continue can only be used inside a loop".to_string());
                }
            }
            Statement::FunctionDef {
                name,
                params,
                body,
                exported,
            } => {
                self.lower_function(name, params, body, *exported)?;
            }
            Statement::Import { alias, .. } => {
                let exports = {
                    let ctx = self.module.context.borrow();
                    ctx.imports.get(&ctx.qualify(alias)).cloned()
                }
                .unwrap_or_default();
                let mut fields = Vec::new();
                for (export_name, slot) in exports {
                    let value = self.fresh_value();
                    self.emit(SsaInstruction::LoadGlobal { dst: value, slot });
                    fields.push((export_name, value));
                }
                let object = self.fresh_value();
                self.emit(SsaInstruction::BuildObject {
                    dst: object,
                    fields,
                });
                let alias_slot = self.module.context.borrow_mut().resolve_global(&alias);
                self.emit(SsaInstruction::StoreGlobal {
                    slot: alias_slot,
                    value: object,
                });
            }
        }
        Ok(())
    }

    fn lower_if(
        &mut self,
        branches: &[(Expression, Vec<Statement>)],
        else_branch: &Option<Vec<Statement>>,
    ) -> Result<(), String> {
        let merge = self.new_block(None);
        let mut branch_ends = Vec::new();
        for (condition, statements) in branches {
            let condition_value = self.lower_expression(condition)?;
            let current_block = self.current;
            let then_block = self.new_block(Some(current_block));
            let else_block = self.new_block(Some(current_block));
            self.block_mut(current_block).terminator = SsaTerminator::Branch {
                condition: condition_value,
                then_block,
                else_block,
            };
            self.current = then_block;
            self.lower_statements(statements)?;
            let then_end = self.current;
            if matches!(
                self.block_mut(then_end).terminator,
                SsaTerminator::Fallthrough
            ) {
                self.block_mut(then_end).terminator = SsaTerminator::Jump(merge);
                branch_ends.push(then_end);
            }
            self.current = else_block;
        }
        if let Some(statements) = else_branch {
            self.lower_statements(statements)?;
        }
        let else_end = self.current;
        if matches!(
            self.block_mut(else_end).terminator,
            SsaTerminator::Fallthrough
        ) {
            self.block_mut(else_end).terminator = SsaTerminator::Jump(merge);
            branch_ends.push(else_end);
        }
        for predecessor in branch_ends {
            self.blocks[merge.0 as usize].predecessors.push(predecessor);
        }
        self.current = merge;
        Ok(())
    }

    fn lookup_variable(&self, name: &str) -> Option<Variable> {
        let stack = self.module.scope_stack.borrow();
        if stack.is_empty() {
            return self
                .module
                .context
                .borrow()
                .lookup_global(name)
                .map(Variable::Global);
        }
        let current = stack.len() - 1;
        if let Some(&slot) = stack[current].locals.get(name) {
            return Some(Variable::Local(slot));
        }
        if let Some(pos) = stack[current].upvalue_names.iter().position(|n| n == name) {
            return Some(Variable::Upvalue(pos as u32));
        }
        if stack[current].is_module {
            return self
                .module
                .context
                .borrow()
                .lookup_global(name)
                .map(Variable::Global);
        }
        for frame in stack[..current].iter().rev() {
            if frame.is_module {
                break;
            }
            if frame.locals.contains_key(name) || frame.upvalue_names.iter().any(|n| n == name) {
                return None;
            }
        }
        self.module
            .context
            .borrow()
            .lookup_global(name)
            .map(Variable::Global)
    }

    fn substitute_expression(
        expression: &Expression,
        params: &[String],
        args: &[Expression],
    ) -> Expression {
        match expression {
            Expression::Identifier(name) => {
                if let Some(position) = params.iter().position(|param| param == name) {
                    return args[position].clone();
                }
                expression.clone()
            }
            Expression::BinaryOp { left, op, right } => Expression::BinaryOp {
                left: Box::new(Self::substitute_expression(left, params, args)),
                op: *op,
                right: Box::new(Self::substitute_expression(right, params, args)),
            },
            Expression::UnaryOp { op, expr } => Expression::UnaryOp {
                op: *op,
                expr: Box::new(Self::substitute_expression(expr, params, args)),
            },
            Expression::MemberAccess { object, member } => Expression::MemberAccess {
                object: Box::new(Self::substitute_expression(object, params, args)),
                member: member.clone(),
            },
            Expression::Index { object, index } => Expression::Index {
                object: Box::new(Self::substitute_expression(object, params, args)),
                index: Box::new(Self::substitute_expression(index, params, args)),
            },
            other => other.clone(),
        }
    }

    fn try_inline_named_call(
        &mut self,
        name: &str,
        args: &[Expression],
    ) -> Result<Option<ValueId>, String> {
        if !matches!(self.lookup_variable(name), Some(Variable::Global(_))) {
            return Ok(None);
        }
        let inline = {
            let ctx = self.module.context.borrow();
            let key = ctx.qualify(name);
            if ctx.inline_blocked.contains(&key) {
                return Ok(None);
            }
            ctx.inline_functions.get(&key).cloned()
        };
        let Some((params, body)) = inline else {
            return Ok(None);
        };
        if params.len() != args.len() || body.len() != 1 {
            return Ok(None);
        }
        let Some(Statement::Return(Some(return_expression))) = body.first() else {
            return Ok(None);
        };
        // The body is lowered in the caller's scope, so it may only use its parameters.
        if !Self::inlinable_body(return_expression, &params) {
            return Ok(None);
        }
        // Arguments are substituted textually, so they must be side-effect free.
        if !args.iter().all(Self::is_trivial_argument) {
            return Ok(None);
        }
        let substituted = Self::substitute_expression(return_expression, &params, args);
        Ok(Some(self.lower_expression(&substituted)?))
    }

    fn static_function_index(&self, name: &str) -> Option<u32> {
        let ctx = self.module.context.borrow();
        let key = ctx.qualify(name);
        if ctx.inline_blocked.contains(&key) {
            return None;
        }
        ctx.function_globals.get(&key).copied()
    }

    fn inlinable_body(expression: &Expression, params: &[String]) -> bool {
        match expression {
            Expression::Literal(
                Literal::Number(_)
                | Literal::Float(_)
                | Literal::String(_)
                | Literal::Boolean(_)
                | Literal::Null,
            ) => true,
            Expression::Identifier(name) => params.iter().any(|param| param == name),
            Expression::BinaryOp { left, right, .. } => {
                Self::inlinable_body(left, params) && Self::inlinable_body(right, params)
            }
            Expression::UnaryOp { expr, .. } => Self::inlinable_body(expr, params),
            Expression::MemberAccess { object, .. } => Self::inlinable_body(object, params),
            Expression::Index { object, index } => {
                Self::inlinable_body(object, params) && Self::inlinable_body(index, params)
            }
            _ => false,
        }
    }

    fn is_trivial_argument(expression: &Expression) -> bool {
        matches!(
            expression,
            Expression::Identifier(_)
                | Expression::Literal(
                    Literal::Number(_)
                        | Literal::Float(_)
                        | Literal::String(_)
                        | Literal::Boolean(_)
                        | Literal::Null
                )
        )
    }

    fn less_const_ident(condition: &Expression) -> Option<(&str, i64)> {
        let Expression::BinaryOp { left, op, right } = condition else {
            return None;
        };
        if *op != BinaryOperator::LessThan {
            return None;
        }
        match (left.as_ref(), right.as_ref()) {
            (Expression::Identifier(name), Expression::Literal(Literal::Number(limit))) => {
                Some((name.as_str(), *limit))
            }
            _ => None,
        }
    }

    fn integer_add_update(name: &str, value: &Expression) -> Option<(i64, bool)> {
        let Expression::BinaryOp { left, op, right } = value else {
            return None;
        };
        if *op != BinaryOperator::Add {
            return None;
        }
        match (left.as_ref(), right.as_ref()) {
            (Expression::Identifier(src), Expression::Literal(Literal::Number(step)))
                if src == name =>
            {
                Some((*step, true))
            }
            (Expression::Literal(Literal::Number(step)), Expression::Identifier(src))
                if src == name =>
            {
                Some((*step, false))
            }
            _ => None,
        }
    }

    fn try_special_assignment(&mut self, name: &str, value: &Expression) -> Result<bool, String> {
        let Some(var) = self.lookup_variable(name) else {
            return Ok(false);
        };
        if let Expression::BinaryOp { left, op, right } = value {
            if *op == BinaryOperator::Add {
                if let (Expression::Identifier(src), Expression::Literal(Literal::String(suffix))) =
                    (left.as_ref(), right.as_ref())
                {
                    if src == name {
                        if let Variable::Global(slot) = var {
                            self.emit(SsaInstruction::AppendStringGlobal {
                                slot,
                                suffix: suffix.clone(),
                            });
                            return Ok(true);
                        }
                    }
                }
            }
        }
        if let Some((step, left_is_ident)) = Self::integer_add_update(name, value) {
            if matches!(var, Variable::Local(_)) && !left_is_ident {
                return Ok(false);
            }
            match var {
                Variable::Global(slot) => {
                    self.emit(SsaInstruction::AddGlobal { slot, delta: step });
                    return Ok(true);
                }
                Variable::Local(slot) => {
                    self.emit(SsaInstruction::AddLocal { slot, delta: step });
                    return Ok(true);
                }
                Variable::Upvalue(_) => return Ok(false),
            }
        }
        if let Expression::BinaryOp { left, op, right } = value {
            if *op == BinaryOperator::Subtract {
                if let (Expression::Identifier(src), Expression::Literal(Literal::Number(amount))) =
                    (left.as_ref(), right.as_ref())
                {
                    if src == name {
                        if let (Variable::Global(slot), Some(delta)) = (var, amount.checked_neg()) {
                            self.emit(SsaInstruction::AddGlobal { slot, delta });
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn try_emit_loop_inc_less(
        &mut self,
        condition: &Expression,
        body: &[Statement],
    ) -> Result<bool, String> {
        if body.len() != 1 {
            return Ok(false);
        }
        let Some((name, limit)) = Self::less_const_ident(condition) else {
            return Ok(false);
        };
        let Statement::Assignment {
            name: assigned,
            value,
        } = &body[0]
        else {
            return Ok(false);
        };
        if assigned != name {
            return Ok(false);
        }
        let Some((step, left_is_ident)) = Self::integer_add_update(name, value) else {
            return Ok(false);
        };
        let Some(var) = self.lookup_variable(name) else {
            return Ok(false);
        };
        let (slot, local) = match var {
            Variable::Global(slot) => (slot, false),
            Variable::Local(slot) if left_is_ident => (slot, true),
            Variable::Local(_) | Variable::Upvalue(_) => return Ok(false),
        };
        self.emit(SsaInstruction::LoopIncLess {
            slot,
            limit,
            step,
            local,
        });
        Ok(true)
    }

    fn lower_while(&mut self, condition: &Expression, body: &[Statement]) -> Result<(), String> {
        if self.try_emit_loop_inc_less(condition, body)? {
            return Ok(());
        }
        let loop_entry = self.current;
        let header = self.new_block(Some(loop_entry));
        if matches!(
            self.block_mut(loop_entry).terminator,
            SsaTerminator::Fallthrough
        ) {
            self.block_mut(loop_entry).terminator = SsaTerminator::Jump(header);
        }
        self.current = header;
        let less_const = Self::less_const_ident(condition).and_then(|(name, limit)| {
            match self.lookup_variable(name)? {
                Variable::Global(slot) => Some((slot, limit, false)),
                Variable::Local(slot) => Some((slot, limit, true)),
                Variable::Upvalue(_) => None,
            }
        });
        let condition_value = if less_const.is_none() {
            Some(self.lower_expression(condition)?)
        } else {
            None
        };
        let body_block = self.new_block(Some(header));
        let exit_block = self.new_block(Some(header));
        self.block_mut(header).terminator = if let Some((slot, limit, local)) = less_const {
            SsaTerminator::CompareLessConst {
                slot,
                limit,
                local,
                then_block: body_block,
                else_block: exit_block,
            }
        } else {
            SsaTerminator::Branch {
                condition: condition_value.expect("lowered loop condition"),
                then_block: body_block,
                else_block: exit_block,
            }
        };
        self.current = body_block;
        self.loop_exits.push((exit_block, header));
        self.lower_statements(body)?;
        self.loop_exits.pop();
        if matches!(self.current_mut().terminator, SsaTerminator::Fallthrough) {
            let body_end = self.current;
            self.block_mut(body_end).terminator = SsaTerminator::Jump(header);
            self.block_mut(header).predecessors.push(body_end);
        }
        self.current = exit_block;
        self.promote_loop_scalars(header, exit_block, loop_entry);
        self.hoist_loop_constants(header, exit_block, loop_entry);
        Ok(())
    }

    fn collect_loop_blocks(&self, header: BlockId, exit: BlockId) -> Vec<BlockId> {
        let mut seen = HashSet::new();
        let mut stack = vec![header];
        while let Some(id) = stack.pop() {
            if id == exit || !seen.insert(id) {
                continue;
            }
            if id.0 as usize >= self.blocks.len() {
                continue;
            }
            match self.blocks[id.0 as usize].terminator {
                SsaTerminator::Jump(target) => stack.push(target),
                SsaTerminator::Branch {
                    then_block,
                    else_block,
                    ..
                }
                | SsaTerminator::CompareLessConst {
                    then_block,
                    else_block,
                    ..
                } => {
                    stack.push(then_block);
                    stack.push(else_block);
                }
                SsaTerminator::Fallthrough
                | SsaTerminator::Return(_)
                | SsaTerminator::TailCall { .. } => {}
            }
        }
        seen.into_iter().collect()
    }

    fn loop_escapes(&self, blocks: &[BlockId]) -> bool {
        blocks.iter().any(|id| {
            self.blocks[id.0 as usize].instructions.iter().any(|inst| {
                matches!(
                    inst,
                    SsaInstruction::Call { .. } | SsaInstruction::MakeClosure { .. }
                )
            })
        })
    }

    fn poison_scalar_use(id: ValueId, produced: &HashMap<u32, u32>, poison: &mut HashSet<u32>) {
        if let Some(slot) = produced.get(&id.0) {
            poison.insert(*slot);
        }
    }

    fn promote_loop_scalars(&mut self, header: BlockId, exit: BlockId, preheader: BlockId) {
        let blocks = self.collect_loop_blocks(header, exit);
        if blocks.is_empty() || self.loop_escapes(&blocks) {
            return;
        }
        let mut produced = HashMap::new();
        let mut seen = HashSet::new();
        let mut poison = HashSet::new();
        for id in &blocks {
            for inst in &self.blocks[id.0 as usize].instructions {
                match inst {
                    SsaInstruction::LoadGlobal { dst, slot } => {
                        produced.insert(dst.0, *slot);
                        seen.insert(*slot);
                    }
                    SsaInstruction::StoreGlobal { slot, .. }
                    | SsaInstruction::AddGlobal { slot, .. } => {
                        seen.insert(*slot);
                    }
                    SsaInstruction::AppendStringGlobal { slot, .. } => {
                        poison.insert(*slot);
                    }
                    SsaInstruction::LoopIncLess {
                        slot, local: false, ..
                    } => {
                        poison.insert(*slot);
                    }
                    SsaInstruction::Call { callee, args, .. } => {
                        Self::poison_scalar_use(*callee, &produced, &mut poison);
                        for arg in args {
                            Self::poison_scalar_use(*arg, &produced, &mut poison);
                        }
                    }
                    SsaInstruction::PropertyGet { object, .. }
                    | SsaInstruction::PropertySet { object, .. }
                    | SsaInstruction::IndexGet { object, .. }
                    | SsaInstruction::IndexSet { object, .. }
                    | SsaInstruction::ArrayLen { object, .. }
                    | SsaInstruction::ArrayPush { object, .. }
                    | SsaInstruction::ArrayPop { object, .. }
                    | SsaInstruction::ObjectKeys { object, .. }
                    | SsaInstruction::ObjectDelete { object, .. }
                    | SsaInstruction::MemberLength { object, .. } => {
                        Self::poison_scalar_use(*object, &produced, &mut poison);
                    }
                    SsaInstruction::ObjectDeleteKey { object, key } => {
                        Self::poison_scalar_use(*object, &produced, &mut poison);
                        Self::poison_scalar_use(*key, &produced, &mut poison);
                    }
                    SsaInstruction::BuildArray { elements, .. } => {
                        for element in elements {
                            Self::poison_scalar_use(*element, &produced, &mut poison);
                        }
                    }
                    SsaInstruction::BuildObject { fields, .. } => {
                        for (_, value) in fields {
                            Self::poison_scalar_use(*value, &produced, &mut poison);
                        }
                    }
                    _ => {}
                }
            }
            if let SsaTerminator::CompareLessConst {
                slot, local: false, ..
            } = self.blocks[id.0 as usize].terminator
            {
                seen.insert(slot);
            }
        }
        let slots: Vec<u32> = seen
            .into_iter()
            .filter(|slot| !poison.contains(slot))
            .collect();
        if slots.is_empty() {
            return;
        }
        let mut mapping = HashMap::new();
        {
            let mut stack = self.module.scope_stack.borrow_mut();
            let Some(frame) = stack.last_mut() else {
                return;
            };
            for slot in &slots {
                let local = frame.allocate_local();
                frame.locals.insert(format!("__promo_{local}"), local);
                mapping.insert(*slot, local);
            }
        }
        for id in &blocks {
            let block = &mut self.blocks[id.0 as usize];
            for inst in &mut block.instructions {
                match inst {
                    SsaInstruction::LoadGlobal { dst, slot } => {
                        if let Some(&local) = mapping.get(slot) {
                            *inst = SsaInstruction::LoadLocal {
                                dst: *dst,
                                slot: local,
                            };
                        }
                    }
                    SsaInstruction::StoreGlobal { slot, value } => {
                        if let Some(&local) = mapping.get(slot) {
                            *inst = SsaInstruction::StoreLocal {
                                slot: local,
                                value: *value,
                            };
                        }
                    }
                    SsaInstruction::AddGlobal { slot, delta } => {
                        if let Some(&local) = mapping.get(slot) {
                            *inst = SsaInstruction::AddLocal {
                                slot: local,
                                delta: *delta,
                            };
                        }
                    }
                    _ => {}
                }
            }
            if let SsaTerminator::CompareLessConst { slot, local, .. } = &mut block.terminator {
                if !*local {
                    if let Some(&mapped) = mapping.get(slot) {
                        *slot = mapped;
                        *local = true;
                    }
                }
            }
        }
        let mut stores = Vec::new();
        for (global, local) in &mapping {
            let loaded = self.fresh_value();
            self.blocks[preheader.0 as usize]
                .instructions
                .push(SsaInstruction::LoadGlobal {
                    dst: loaded,
                    slot: *global,
                });
            self.blocks[preheader.0 as usize]
                .instructions
                .push(SsaInstruction::StoreLocal {
                    slot: *local,
                    value: loaded,
                });
            let stored = self.fresh_value();
            stores.push(SsaInstruction::LoadLocal {
                dst: stored,
                slot: *local,
            });
            stores.push(SsaInstruction::StoreGlobal {
                slot: *global,
                value: stored,
            });
        }
        for (offset, inst) in stores.iter().cloned().enumerate() {
            self.blocks[exit.0 as usize]
                .instructions
                .insert(offset, inst);
        }
        for id in &blocks {
            let positions: Vec<usize> = self.blocks[id.0 as usize]
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, inst)| {
                    matches!(inst, SsaInstruction::Return { .. }).then_some(index)
                })
                .collect();
            if positions.is_empty() {
                if matches!(
                    self.blocks[id.0 as usize].terminator,
                    SsaTerminator::Return(_)
                ) {
                    self.blocks[id.0 as usize]
                        .instructions
                        .extend(stores.iter().cloned());
                }
                continue;
            }
            for pos in positions.into_iter().rev() {
                for (offset, inst) in stores.iter().cloned().enumerate() {
                    self.blocks[id.0 as usize]
                        .instructions
                        .insert(pos + offset, inst);
                }
            }
        }
    }

    fn hoist_loop_constants(&mut self, header: BlockId, exit: BlockId, preheader: BlockId) {
        let blocks = self.collect_loop_blocks(header, exit);
        let mut sites = Vec::new();
        for id in &blocks {
            for (index, inst) in self.blocks[id.0 as usize].instructions.iter().enumerate() {
                if let SsaInstruction::Const { dst, idx } = inst {
                    let hoistable = matches!(
                        self.constants.get(*idx as usize),
                        Some(Value::Number(_) | Value::Boolean(_) | Value::Null)
                    );
                    if hoistable {
                        sites.push((*id, index, *idx, *dst));
                    }
                }
            }
        }
        let mut hoisted = HashMap::new();
        for (block, index, idx, dst) in sites {
            let src = *hoisted.entry(idx).or_insert_with(|| self.fresh_value());
            self.blocks[block.0 as usize].instructions[index] = SsaInstruction::Move { dst, src };
        }
        for (idx, dst) in hoisted {
            self.blocks[preheader.0 as usize]
                .instructions
                .push(SsaInstruction::Const { dst, idx });
        }
    }

    fn lower_for_in(
        &mut self,
        key_name: &str,
        value_name: Option<&str>,
        iterable: &Expression,
        body: &[Statement],
    ) -> Result<(), String> {
        let iterable_value = self.lower_expression(iterable)?;
        let zero_idx = self.add_constant(Value::Number(0));
        let (index_slot, key_slot, value_slot) = {
            let mut stack = self.module.scope_stack.borrow_mut();
            let current = stack.len() - 1;
            let index_slot = stack[current].allocate_local();
            stack[current]
                .locals
                .insert(format!("__for_index_{}", key_name), index_slot);
            let key_slot = stack[current].allocate_local();
            stack[current].locals.insert(key_name.to_string(), key_slot);
            let value_slot = if let Some(v_name) = value_name {
                let slot = stack[current].allocate_local();
                stack[current].locals.insert(v_name.to_string(), slot);
                slot
            } else {
                key_slot
            };
            (index_slot, key_slot, value_slot)
        };
        let zero_value = self.fresh_value();
        self.emit(SsaInstruction::Const {
            dst: zero_value,
            idx: zero_idx,
        });
        self.emit(SsaInstruction::StoreLocal {
            slot: index_slot,
            value: zero_value,
        });

        let collection_value = if value_name.is_some() {
            let keys = self.fresh_value();
            self.emit(SsaInstruction::ObjectKeys {
                dst: keys,
                object: iterable_value,
            });
            keys
        } else {
            iterable_value
        };

        let header = self.new_block(Some(self.current));
        if matches!(
            self.block_mut(self.current).terminator,
            SsaTerminator::Fallthrough
        ) {
            self.block_mut(self.current).terminator = SsaTerminator::Jump(header);
        }
        self.current = header;
        let exit_block = self.new_block(Some(header));
        let body_block = self.new_block(Some(header));
        let index_value = self.fresh_value();
        self.emit(SsaInstruction::LoadLocal {
            dst: index_value,
            slot: index_slot,
        });
        let collection_len = self.fresh_value();
        self.emit(SsaInstruction::ArrayLen {
            dst: collection_len,
            object: collection_value,
        });
        let condition = self.fresh_value();
        self.emit(SsaInstruction::Binary {
            dst: condition,
            op: crate::ast::BinaryOperator::LessThan,
            left: index_value,
            right: collection_len,
        });
        self.current_mut().terminator = SsaTerminator::Branch {
            condition,
            then_block: body_block,
            else_block: exit_block,
        };

        self.current = body_block;
        let item_value = self.fresh_value();
        self.emit(SsaInstruction::IndexGet {
            dst: item_value,
            object: collection_value,
            index: index_value,
        });
        self.emit(SsaInstruction::StoreLocal {
            slot: key_slot,
            value: item_value,
        });
        if value_name.is_some() {
            let value = self.fresh_value();
            self.emit(SsaInstruction::IndexGet {
                dst: value,
                object: iterable_value,
                index: item_value,
            });
            self.emit(SsaInstruction::StoreLocal {
                slot: value_slot,
                value,
            });
        }

        let continue_block = self.new_block(Some(body_block));
        self.current = body_block;
        self.loop_exits.push((exit_block, continue_block));
        self.lower_statements(body)?;
        self.loop_exits.pop();

        let continue_entry = self.current;
        if matches!(
            self.block_mut(continue_entry).terminator,
            SsaTerminator::Fallthrough
        ) {
            self.block_mut(continue_entry).terminator = SsaTerminator::Jump(continue_block);
        }
        self.current = continue_block;
        let current_index = self.fresh_value();
        self.emit(SsaInstruction::LoadLocal {
            dst: current_index,
            slot: index_slot,
        });
        let one = self.add_constant(Value::Number(1));
        let one_value = self.fresh_value();
        self.emit(SsaInstruction::Const {
            dst: one_value,
            idx: one,
        });
        let next_index = self.fresh_value();
        self.emit(SsaInstruction::Binary {
            dst: next_index,
            op: crate::ast::BinaryOperator::Add,
            left: current_index,
            right: one_value,
        });
        self.emit(SsaInstruction::StoreLocal {
            slot: index_slot,
            value: next_index,
        });
        self.current_mut().terminator = SsaTerminator::Jump(header);
        self.block_mut(header).predecessors.push(continue_block);
        self.current = exit_block;
        Ok(())
    }

    fn lower_function(
        &mut self,
        name: &str,
        params: &[String],
        body: &[Statement],
        _exported: bool,
    ) -> Result<(), String> {
        let function_index = {
            let mut ctx = self.module.context.borrow_mut();
            let idx = ctx.next_function;
            ctx.next_function += 1;
            idx
        };

        {
            let mut stack = self.module.scope_stack.borrow_mut();
            let mut frame_locals = HashMap::new();
            for (slot, param) in params.iter().enumerate() {
                frame_locals.insert(param.clone(), slot as u32);
            }
            stack.push(ScopeFrame {
                locals: frame_locals,
                next_local: params.len() as u32,
                upvalue_names: Vec::new(),
                captures: Vec::new(),
                is_module: false,
            });
        }

        let mut function = FunctionLowerer {
            module: self.module.clone(),
            current: BlockId(0),
            next_value: 0,
            next_block: 1,
            blocks: vec![SsaBlock::new(BlockId(0))],
            loop_exits: Vec::new(),
            constants: Vec::new(),
            params: params.to_vec(),
            is_module: false,
        };
        for (slot, _) in params.iter().enumerate() {
            let value = function.fresh_value();
            function.emit(SsaInstruction::LoadLocal {
                dst: value,
                slot: slot as u32,
            });
        }
        function.lower_statements(body)?;
        if !matches!(function.current_mut().terminator, SsaTerminator::Return(_)) {
            let null = function.fresh_value();
            function.emit(SsaInstruction::Null { dst: null });
            function.current_mut().terminator = SsaTerminator::Return(Some(null));
        }

        let mut ssa_function = function.finish_function();
        let frame = self.module.scope_stack.borrow_mut().pop().unwrap();
        ssa_function.name = Some(name.to_string());
        ssa_function.captures = frame.captures.clone();

        {
            let mut ctx = self.module.context.borrow_mut();
            if ctx.module_functions.len() <= function_index as usize {
                ctx.module_functions
                    .resize(function_index as usize + 1, ssa_function.clone());
            }
            ctx.module_functions[function_index as usize] = ssa_function;
            if self.is_module {
                let key = ctx.qualify(name);
                if ctx.function_globals.contains_key(&key) {
                    ctx.inline_blocked.insert(key.clone());
                }
                ctx.function_globals.insert(key, function_index);
            }
            if self.is_module && body.len() == 1 {
                if let Statement::Return(Some(_)) = &body[0] {
                    let key = ctx.qualify(name);
                    ctx.inline_functions
                        .insert(key, (params.to_vec(), body.to_vec()));
                }
            }
        }

        let dst = self.fresh_value();
        self.emit(SsaInstruction::MakeClosure {
            dst,
            function: function_index,
            captures: frame.captures,
        });
        let global = self.module.context.borrow_mut().resolve_global(name);
        self.emit(SsaInstruction::StoreGlobal {
            slot: global,
            value: dst,
        });
        Ok(())
    }

    fn lower_expression(&mut self, expression: &Expression) -> Result<ValueId, String> {
        match expression {
            Expression::Literal(Literal::Number(value)) => {
                let dst = self.fresh_value();
                let idx = self.add_constant(Value::Number(*value));
                self.emit(SsaInstruction::Const { dst, idx });
                Ok(dst)
            }
            Expression::Literal(Literal::Float(value)) => {
                let dst = self.fresh_value();
                let idx = self.add_constant(Value::Float(*value));
                self.emit(SsaInstruction::Const { dst, idx });
                Ok(dst)
            }
            Expression::Literal(Literal::String(value)) => {
                let dst = self.fresh_value();
                let idx = self.add_constant(Value::String(crate::vm::StringValue::from(
                    std::sync::Arc::from(value.as_str()),
                )));
                self.emit(SsaInstruction::Const { dst, idx });
                Ok(dst)
            }
            Expression::Literal(Literal::Boolean(value)) => {
                let dst = self.fresh_value();
                let idx = self.add_constant(Value::Boolean(*value));
                self.emit(SsaInstruction::Const { dst, idx });
                Ok(dst)
            }
            Expression::Literal(Literal::Null) => {
                let dst = self.fresh_value();
                self.emit(SsaInstruction::Null { dst });
                Ok(dst)
            }
            Expression::Literal(Literal::Array { elements, mutable }) => {
                let mut values = Vec::new();
                for element in elements {
                    values.push(self.lower_expression(element)?);
                }
                let dst = self.fresh_value();
                self.emit(SsaInstruction::BuildArray {
                    dst,
                    elements: values,
                    mutable: *mutable,
                });
                Ok(dst)
            }
            Expression::Literal(Literal::Object { fields }) => {
                let mut values = Vec::new();
                for (name, expression) in fields {
                    values.push((name.clone(), self.lower_expression(expression)?));
                }
                let dst = self.fresh_value();
                self.emit(SsaInstruction::BuildObject {
                    dst,
                    fields: values,
                });
                Ok(dst)
            }
            Expression::Identifier(name) => {
                let dst = self.fresh_value();
                match self.resolve_variable(name) {
                    Variable::Global(slot) => {
                        self.emit(SsaInstruction::LoadGlobal { dst, slot });
                    }
                    Variable::Local(slot) => {
                        self.emit(SsaInstruction::LoadLocal { dst, slot });
                    }
                    Variable::Upvalue(slot) => {
                        self.emit(SsaInstruction::LoadUpvalue { dst, slot });
                    }
                }
                Ok(dst)
            }
            Expression::UnaryOp { op, expr } => {
                let value = self.lower_expression(expr)?;
                let dst = self.fresh_value();
                self.emit(SsaInstruction::Unary {
                    dst,
                    op: *op,
                    value,
                });
                Ok(dst)
            }
            Expression::BinaryOp { left, op, right } => {
                let left_literal = match left.as_ref() {
                    Expression::Literal(literal) => Some(literal),
                    _ => None,
                };
                let right_literal = match right.as_ref() {
                    Expression::Literal(literal) => Some(literal),
                    _ => None,
                };
                let left = self.lower_expression(left)?;
                let right = self.lower_expression(right)?;
                let dst = self.fresh_value();
                let operands_are_int_literals = matches!(
                    (left_literal, right_literal),
                    (Some(Literal::Number(_)), Some(Literal::Number(_)))
                );
                if operands_are_int_literals {
                    self.emit(SsaInstruction::BinaryInt {
                        dst,
                        op: *op,
                        left,
                        right,
                    });
                } else {
                    self.emit(SsaInstruction::Binary {
                        dst,
                        op: *op,
                        left,
                        right,
                    });
                }
                Ok(dst)
            }
            Expression::FunctionCall { name, args } => {
                match name.as_str() {
                    "push" | "pop" | "rmv" | "type" | "wait" => {
                        if name == "push" && args.len() == 2 {
                            let object = self.lower_expression(&args[0])?;
                            let value = self.lower_expression(&args[1])?;
                            self.emit(SsaInstruction::ArrayPush { object, value });
                            return Ok(object);
                        }
                        if name == "pop" && args.len() == 1 {
                            let object = self.lower_expression(&args[0])?;
                            let dst = self.fresh_value();
                            self.emit(SsaInstruction::ArrayPop { dst, object });
                            return Ok(dst);
                        }
                        if name == "rmv" && args.len() == 1 {
                            match &args[0] {
                                Expression::MemberAccess { object, member } => {
                                    let object = self.lower_expression(object)?;
                                    self.emit(SsaInstruction::ObjectDelete {
                                        object,
                                        member: member.clone(),
                                    });
                                    let null = self.fresh_value();
                                    self.emit(SsaInstruction::Null { dst: null });
                                    return Ok(null);
                                }
                                Expression::Index { object, index } => {
                                    let object = self.lower_expression(object)?;
                                    let key = self.lower_expression(index)?;
                                    self.emit(SsaInstruction::ObjectDeleteKey { object, key });
                                    let null = self.fresh_value();
                                    self.emit(SsaInstruction::Null { dst: null });
                                    return Ok(null);
                                }
                                _ => {}
                            }
                        }
                        if name == "type" && args.len() == 1 {
                            let value = self.lower_expression(&args[0])?;
                            let dst = self.fresh_value();
                            self.emit(SsaInstruction::Type { dst, value });
                            return Ok(dst);
                        }
                        if name == "wait" && args.len() == 1 {
                            let value = self.lower_expression(&args[0])?;
                            let dst = self.fresh_value();
                            self.emit(SsaInstruction::Wait { dst, value });
                            return Ok(dst);
                        }
                    }
                    _ => {}
                }
                let callee_var = self.resolve_variable(name);
                if let Some(inlined) = self.try_inline_named_call(name, args)? {
                    return Ok(inlined);
                }
                let callee = self.fresh_value();
                match callee_var {
                    Variable::Global(slot) => {
                        self.emit(SsaInstruction::LoadGlobal { dst: callee, slot });
                    }
                    Variable::Local(slot) => {
                        self.emit(SsaInstruction::LoadLocal { dst: callee, slot });
                    }
                    Variable::Upvalue(slot) => {
                        self.emit(SsaInstruction::LoadUpvalue { dst: callee, slot });
                    }
                }
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.lower_expression(arg)?);
                }
                let dst = self.fresh_value();
                let function_index = if let Variable::Global(_) = callee_var {
                    self.static_function_index(name)
                } else {
                    None
                };
                if let Some(function_index) = function_index {
                    self.emit(SsaInstruction::CallDirect {
                        dst,
                        function_index,
                        args: values,
                    });
                } else {
                    self.emit(SsaInstruction::Call {
                        dst,
                        callee,
                        args: values,
                    });
                }
                Ok(dst)
            }
            Expression::Call { callee, args } => {
                let callee = self.lower_expression(callee)?;
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.lower_expression(arg)?);
                }
                let dst = self.fresh_value();
                self.emit(SsaInstruction::Call {
                    dst,
                    callee,
                    args: values,
                });
                Ok(dst)
            }
            Expression::Input { types, prompt } => {
                let prompt = self.lower_expression(prompt)?;
                let mut mask = 0u8;
                for input_type in types {
                    match input_type {
                        crate::ast::InputType::String => mask |= 0x01,
                        crate::ast::InputType::Int => mask |= 0x02,
                        crate::ast::InputType::Float => mask |= 0x04,
                    }
                }
                let dst = self.fresh_value();
                self.emit(SsaInstruction::Input {
                    dst,
                    prompt,
                    type_mask: mask,
                });
                Ok(dst)
            }
            Expression::Index { object, index } => {
                let object = self.lower_expression(object)?;
                let index = self.lower_expression(index)?;
                let dst = self.fresh_value();
                self.emit(SsaInstruction::IndexGet { dst, object, index });
                Ok(dst)
            }
            Expression::MemberAccess { object, member } => {
                let object = self.lower_expression(object)?;
                let dst = self.fresh_value();
                if member == "length" {
                    self.emit(SsaInstruction::MemberLength { dst, object });
                } else {
                    self.emit(SsaInstruction::PropertyGet {
                        dst,
                        object,
                        member: member.clone(),
                    });
                }
                Ok(dst)
            }
            Expression::FunctionExpr { params, body } => {
                let function_index = {
                    let mut ctx = self.module.context.borrow_mut();
                    let idx = ctx.next_function;
                    ctx.next_function += 1;
                    idx
                };

                {
                    let mut stack = self.module.scope_stack.borrow_mut();
                    let mut frame_locals = HashMap::new();
                    for (slot, param) in params.iter().enumerate() {
                        frame_locals.insert(param.clone(), slot as u32);
                    }
                    stack.push(ScopeFrame {
                        locals: frame_locals,
                        next_local: params.len() as u32,
                        upvalue_names: Vec::new(),
                        captures: Vec::new(),
                        is_module: false,
                    });
                }

                let mut function = FunctionLowerer {
                    module: self.module.clone(),
                    current: BlockId(0),
                    next_value: 0,
                    next_block: 1,
                    blocks: vec![SsaBlock::new(BlockId(0))],
                    loop_exits: Vec::new(),
                    constants: Vec::new(),
                    params: params.to_vec(),
                    is_module: false,
                };
                for (slot, _) in params.iter().enumerate() {
                    let value = function.fresh_value();
                    function.emit(SsaInstruction::LoadLocal {
                        dst: value,
                        slot: slot as u32,
                    });
                }
                function.lower_statements(body)?;
                if !matches!(function.current_mut().terminator, SsaTerminator::Return(_)) {
                    let null = function.fresh_value();
                    function.emit(SsaInstruction::Null { dst: null });
                    function.current_mut().terminator = SsaTerminator::Return(Some(null));
                }

                let mut ssa_function = function.finish_function();
                let frame = self.module.scope_stack.borrow_mut().pop().unwrap();
                ssa_function.name = Some(format!("<lambda_{}>", function_index));
                ssa_function.captures = frame.captures.clone();

                {
                    let mut ctx = self.module.context.borrow_mut();
                    if ctx.module_functions.len() <= function_index as usize {
                        ctx.module_functions
                            .resize(function_index as usize + 1, ssa_function.clone());
                    }
                    ctx.module_functions[function_index as usize] = ssa_function;
                }

                let dst = self.fresh_value();
                self.emit(SsaInstruction::MakeClosure {
                    dst,
                    function: function_index,
                    captures: frame.captures,
                });
                Ok(dst)
            }
        }
    }

    fn finish(&mut self) -> SsaFunction {
        let max_registers = self.next_value;
        SsaFunction {
            name: Some("<module>".to_string()),
            params: self.params.clone(),
            captures: Vec::new(),
            blocks: self.blocks.clone(),
            max_registers: max_registers + 32,
            local_count: 0,
            constants: self.constants.clone(),
        }
    }

    fn finish_function(&mut self) -> SsaFunction {
        let local_count = self
            .module
            .scope_stack
            .borrow()
            .last()
            .map(|frame| frame.next_local)
            .unwrap_or(0);
        let mut function = self.finish();
        function.local_count = local_count;
        function
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::RegisterVM;

    #[test]
    fn register_backend_can_lower_and_execute_arithmetic() {
        let mut lexer = Lexer::new("set x = 10\nset y = x + 5\nprint(y)\n");
        let statements = Parser::new(lexer.tokenize()).parse().unwrap();
        let mut lowerer = ModuleLowerer::new();
        let module = lowerer.lower_module(&statements).unwrap();
        RegisterVM::new().execute(&module).unwrap();
        assert!(!module.entry.blocks.is_empty());
        assert!(module
            .constants
            .iter()
            .any(|value| matches!(value, Value::Number(10))));
    }

    fn global_value(source: &str, name: &str) -> Value {
        let statements = Parser::new(Lexer::new(source).tokenize()).parse().unwrap();
        let mut lowerer = ModuleLowerer::new();
        let module = lowerer.lower_module(&statements).unwrap();
        let mut vm = RegisterVM::new();
        vm.execute(&module).unwrap();
        let slot = module.globals[name] as usize;
        vm.global(slot)
    }

    #[test]
    fn register_arithmetic_loop_matches_accumulator() {
        let value = global_value(
            "set result = 0\nset i = 0\nwhile (i < 10) {\nset result = result + i * 3 + 1\nset i = i + 1\n}\n",
            "result",
        );
        assert!(matches!(value, Value::Number(145)));
    }

    #[test]
    fn register_string_append_keeps_length() {
        let value = global_value(
            "set text = \"\"\nset i = 0\nwhile (i < 4) {\nset text = text + \"x\"\nset i = i + 1\n}\n",
            "text",
        );
        match value {
            Value::String(text) => assert_eq!(text.as_str(), "xxxx"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn register_direct_call_and_inline_return_expression() {
        let value = global_value(
            "fn add_three(x) {\nreturn x + 3\n}\nset out = add_three(9)\n",
            "out",
        );
        assert!(matches!(value, Value::Number(12)));
    }

    #[test]
    fn register_tail_call_reuses_register_frame() {
        let value = global_value(
            "fn down(n) {\nif (n < 1) { return 0 }\nreturn down(n - 1)\n}\nset out = down(5)\n",
            "out",
        );
        assert!(matches!(value, Value::Number(0)));
    }

    #[test]
    fn register_function_call_sums_arguments() {
        let value = global_value(
            "fn add(a, b) {\nreturn a + b\n}\nset sum = 0\nset i = 0\nwhile (i < 4) {\nset sum = sum + add(i, 3)\nset i = i + 1\n}\n",
            "sum",
        );
        assert!(matches!(value, Value::Number(18)));
    }

    #[test]
    fn register_closure_reads_captured_factor() {
        let value = global_value(
            "set factor = 3\nset multiply = fn(x) {\nreturn x * factor\n}\nset out = multiply(4)\n",
            "out",
        );
        assert!(matches!(value, Value::Number(12)));
    }

    #[test]
    fn register_break_stops_counter() {
        let value = global_value(
            "set i = 0\nwhile (i < 10) {\nset i = i + 1\nif (i == 3) {\nbreak\n}\n}\n",
            "i",
        );
        assert!(matches!(value, Value::Number(3)));
    }

    #[test]
    fn register_closure_updates_captured_global() {
        let value = global_value(
            "set n = 1\nset inc = fn() {\nset n = n + 1\n}\ninc()\n",
            "n",
        );
        assert!(matches!(value, Value::Number(2)));
    }

    #[test]
    fn float_literal_is_not_merged_with_equal_integer_constant() {
        let value = global_value("set a = 7 / 2\nset b = 7.0 / 2\n", "b");
        assert!(matches!(value, Value::Float(f) if f == 3.5));
    }

    #[test]
    fn integer_literal_is_not_merged_with_equal_float_constant() {
        let value = global_value("set a = 1.0\nset b = 1 == 1\n", "b");
        assert!(matches!(value, Value::Boolean(true)));
        let zero = global_value("set a = 0\nset b = 0.0\n", "b");
        assert!(matches!(zero, Value::Float(_)));
    }

    #[test]
    fn inlined_function_reads_global_not_caller_local() {
        let value = global_value(
            "set factor = 10\nfn scale(x) { return x * factor }\nfn g() {\nset factor = 2\nreturn scale(5)\n}\nset r = g()\n",
            "r",
        );
        assert!(matches!(value, Value::Number(50)));
    }

    #[test]
    fn inlined_function_evaluates_side_effecting_argument_once() {
        let source = "set c = 0\nfn tick() {\nset c = c + 1\nreturn c\n}\nfn sq(n) { return n * n }\nset r = sq(tick())\n";
        assert!(matches!(global_value(source, "r"), Value::Number(1)));
        assert!(matches!(global_value(source, "c"), Value::Number(1)));
    }

    #[test]
    fn parameter_shadowing_a_global_function_is_called() {
        let value = global_value(
            "fn sq(n) { return n * n }\nfn apply(sq, x) { return sq(x) }\nset r = apply(fn(v) { return v + 1 }, 5)\n",
            "r",
        );
        assert!(matches!(value, Value::Number(6)));
    }

    #[test]
    fn nested_call_arguments_use_callee_parameters() {
        let value = global_value(
            "fn g(x) { return x * 100 }\nfn f(x) { return g(x) + 1 }\nfn caller() {\nset x = 7\nreturn f(2)\n}\nset r = caller()\n",
            "r",
        );
        assert!(matches!(value, Value::Number(201)));
    }

    #[test]
    fn redefined_function_is_not_inlined() {
        let value = global_value(
            "fn h(x) { return x + 1 }\nfn call_h() { return h(1) }\nfn h(x) { return x + 100 }\nset r = call_h()\n",
            "r",
        );
        assert!(matches!(value, Value::Number(101)));
    }

    fn vm_error(source: &str) -> String {
        let statements = Parser::new(Lexer::new(source).tokenize()).parse().unwrap();
        let module = ModuleLowerer::new().lower_module(&statements).unwrap();
        RegisterVM::new().execute(&module).unwrap_err()
    }

    #[test]
    fn calling_non_function_is_an_error() {
        assert_eq!(
            vm_error("set x = 5\nx(1)\n"),
            "Attempted to call non-function value"
        );
        assert_eq!(
            vm_error("set r = undefined_function(1)\n"),
            "Attempted to call non-function value"
        );
    }

    #[test]
    fn integer_division_by_zero_is_an_error() {
        assert_eq!(vm_error("set q = 1 / 0\n"), "Division by zero");
        assert_eq!(vm_error("set z = 0\nset q = 10 / z\n"), "Division by zero");
    }

    #[test]
    fn min_divided_by_minus_one_wraps_instead_of_panicking() {
        let value = global_value(
            "set m = -9223372036854775807 - 1\nset n = -1\nset q = m / n\n",
            "q",
        );
        assert!(matches!(value, Value::Number(i64::MIN)));
        let literal = global_value("set q = (-9223372036854775807 - 1) / -1\n", "q");
        assert!(matches!(literal, Value::Number(i64::MIN)));
    }

    fn with_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(512 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn recursion_up_to_the_limit_succeeds() {
        let depth = crate::vm::MAX_CALL_DEPTH - 1;
        let ok = with_large_stack(move || {
            let source = format!(
                "fn f(n) {{\nif (n == 0) {{ return 0 }}\nreturn 1 + f(n - 1)\n}}\nset r = f({})\n",
                depth
            );
            matches!(global_value(&source, "r"), Value::Number(n) if n == depth as i64)
        });
        assert!(ok);
    }

    #[test]
    fn recursion_past_the_limit_is_a_runtime_error() {
        let error = with_large_stack(|| {
            vm_error(
                "fn f(n) {\nif (n == 0) { return 0 }\nreturn 1 + f(n - 1)\n}\nset r = f(100000)\n",
            )
        });
        assert_eq!(error, crate::vm::recursion_limit_message());
    }

    #[test]
    fn call_depth_is_released_after_returning() {
        let ok = with_large_stack(|| {
            let source = "fn f(n) {\nif (n == 0) { return 0 }\nreturn 1 + f(n - 1)\n}\n\
                          set i = 0\nset total = 0\nwhile (i < 20) {\nset total = total + f(900)\nset i = i + 1\n}\n";
            matches!(global_value(source, "total"), Value::Number(18000))
        });
        assert!(ok);
    }
}
