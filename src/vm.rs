use crate::bytecode::*;
use crate::ast::BinaryOperator;
use indexmap::IndexMap;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone)]
pub struct StringValue(Arc<String>);

impl StringValue {
    fn new(value: impl Into<String>) -> Self {
        Self(Arc::new(value.into()))
    }

    pub(crate) fn concat(left: &Self, right: &Self) -> Self {
        let mut value = String::with_capacity(left.0.len() + right.0.len());
        value.push_str(&left.0);
        value.push_str(&right.0);
        Self(Arc::new(value))
    }

    fn append(&mut self, suffix: &str) {
        if let Some(value) = Arc::get_mut(&mut self.0) {
            value.push_str(suffix);
        } else {
            let mut value = String::with_capacity(self.0.len() + suffix.len());
            value.push_str(&self.0);
            value.push_str(suffix);
            self.0 = Arc::new(value);
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn to_arc_str(&self) -> Arc<str> {
        Arc::from(self.0.as_str())
    }
}

impl From<Arc<str>> for StringValue {
    fn from(value: Arc<str>) -> Self {
        Self::new(value.as_ref())
    }
}

impl From<String> for StringValue {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for StringValue {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl PartialEq for StringValue {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl std::fmt::Debug for StringValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for StringValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub enum ArrayElements {
    Mutable(RefCell<Vec<Value>>),
    Immutable(Vec<Value>),
}

#[derive(Debug, Clone)]
pub struct ArrayData {
    pub mutable: bool,
    pub elements: ArrayElements,
}

impl ArrayData {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        match &self.elements {
            ArrayElements::Mutable(c) => c.borrow().len(),
            ArrayElements::Immutable(v) => v.len(),
        }
    }

    #[inline]
    pub(crate) fn get_cloned(&self, idx: usize) -> Option<Value> {
        match &self.elements {
            ArrayElements::Mutable(c) => c.borrow().get(idx).cloned(),
            ArrayElements::Immutable(v) => v.get(idx).cloned(),
        }
    }

    #[inline]
    pub(crate) fn set_at(&self, idx: usize, value: Value) -> Result<(), String> {
        match &self.elements {
            ArrayElements::Mutable(c) => {
                let mut elems = c.borrow_mut();
                if idx >= elems.len() {
                    return Err(format!("Array index {} out of bounds", idx));
                }
                elems[idx] = value;
                Ok(())
            }
            ArrayElements::Immutable(_) => Err("Cannot modify immutable array".to_string()),
        }
    }

    #[inline]
    pub(crate) fn push(&self, value: Value) -> Result<(), String> {
        match &self.elements {
            ArrayElements::Mutable(c) => {
                c.borrow_mut().push(value);
                Ok(())
            }
            ArrayElements::Immutable(_) => Err("Cannot push to immutable array".to_string()),
        }
    }

    #[inline]
    pub(crate) fn pop(&self) -> Result<Value, String> {
        match &self.elements {
            ArrayElements::Mutable(c) => {
                let mut elems = c.borrow_mut();
                if elems.is_empty() {
                    return Err("Cannot pop from empty array".to_string());
                }
                Ok(elems.pop().unwrap())
            }
            ArrayElements::Immutable(_) => Err("Cannot pop from immutable array".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Number(i64),
    Float(f64),
    String(StringValue),
    Boolean(bool),
    Null,
    Function {
        bytecode_offset: usize,
        num_params: u32,
        upvalues: Rc<RefCell<Vec<Value>>>,
    },
    RegisterFunction {
        function_index: usize,
        captures: Rc<RefCell<Vec<Value>>>,
    },
    Array(Rc<ArrayData>),
    Object(Rc<ObjectData>),
}

#[derive(Clone, Copy)]
pub(crate) enum NumericOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericError {
    NotNumeric,
    DivisionByZero,
}

pub(crate) fn apply_numeric_op(
    op: NumericOp,
    left: &Value,
    right: &Value,
) -> Result<Value, NumericError> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => match op {
            NumericOp::Add => Ok(Value::Number(left + right)),
            NumericOp::Subtract => Ok(Value::Number(left - right)),
            NumericOp::Multiply => Ok(Value::Number(left * right)),
            NumericOp::Divide if *right == 0 => Err(NumericError::DivisionByZero),
            NumericOp::Divide => Ok(Value::Number(left / right)),
            NumericOp::Equals => Ok(Value::Boolean(left == right)),
            NumericOp::NotEquals => Ok(Value::Boolean(left != right)),
            NumericOp::Greater => Ok(Value::Boolean(left > right)),
            NumericOp::Less => Ok(Value::Boolean(left < right)),
            NumericOp::GreaterEqual => Ok(Value::Boolean(left >= right)),
            NumericOp::LessEqual => Ok(Value::Boolean(left <= right)),
        },
        (Value::Number(left), Value::Float(right)) => {
            apply_mixed_numeric_op(op, *left, *right, true)
        }
        (Value::Float(left), Value::Number(right)) => {
            apply_mixed_numeric_op(op, *right, *left, false)
        }
        (Value::Float(left), Value::Float(right)) => apply_float_op(op, *left, *right),
        _ => Err(NumericError::NotNumeric),
    }
}

#[inline(always)]
fn apply_register_int_op(
    op: crate::ast::BinaryOperator,
    left: i64,
    right: i64,
) -> Result<Value, NumericError> {
    match op {
        BinaryOperator::Add => Ok(Value::Number(left.wrapping_add(right))),
        BinaryOperator::Subtract => Ok(Value::Number(left.wrapping_sub(right))),
        BinaryOperator::Multiply => Ok(Value::Number(left.wrapping_mul(right))),
        BinaryOperator::Divide if right == 0 => Err(NumericError::DivisionByZero),
        BinaryOperator::Divide => Ok(Value::Number(left / right)),
        BinaryOperator::Equals => Ok(Value::Boolean(left == right)),
        BinaryOperator::NotEquals => Ok(Value::Boolean(left != right)),
        BinaryOperator::GreaterThan => Ok(Value::Boolean(left > right)),
        BinaryOperator::LessThan => Ok(Value::Boolean(left < right)),
        BinaryOperator::GreaterOrEqual => Ok(Value::Boolean(left >= right)),
        BinaryOperator::LessOrEqual => Ok(Value::Boolean(left <= right)),
        BinaryOperator::And | BinaryOperator::Or => Err(NumericError::NotNumeric),
    }
}


fn apply_mixed_numeric_op(
    op: NumericOp,
    integer: i64,
    float: f64,
    integer_is_left: bool,
) -> Result<Value, NumericError> {
    match op {
        NumericOp::Add | NumericOp::Subtract | NumericOp::Multiply | NumericOp::Divide => {
            if integer_is_left {
                apply_float_op(op, integer as f64, float)
            } else {
                apply_float_op(op, float, integer as f64)
            }
        }
        _ => {
            let ordering = compare_i64_f64(integer, float);
            let ordering = if integer_is_left {
                ordering
            } else {
                ordering.map(std::cmp::Ordering::reverse)
            };
            Ok(apply_comparison_op(op, ordering))
        }
    }
}

fn compare_i64_f64(integer: i64, float: f64) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;

    const I64_UPPER_BOUND: f64 = 9_223_372_036_854_775_808.0;
    const I64_LOWER_BOUND: f64 = -9_223_372_036_854_775_808.0;

    if float.is_nan() {
        return None;
    }
    if float >= I64_UPPER_BOUND {
        return Some(Ordering::Less);
    }
    if float < I64_LOWER_BOUND {
        return Some(Ordering::Greater);
    }
    if float == I64_LOWER_BOUND {
        return Some(integer.cmp(&i64::MIN));
    }

    let truncated = float as i64;
    match integer.cmp(&truncated) {
        Ordering::Equal if float.fract() > 0.0 => Some(Ordering::Less),
        Ordering::Equal if float.fract() < 0.0 => Some(Ordering::Greater),
        ordering => Some(ordering),
    }
}

fn float_less_than_i64(float: f64, integer: i64) -> bool {
    compare_i64_f64(integer, float) == Some(std::cmp::Ordering::Greater)
}

fn apply_comparison_op(op: NumericOp, ordering: Option<std::cmp::Ordering>) -> Value {
    use std::cmp::Ordering;

    let result = match op {
        NumericOp::Equals => ordering == Some(Ordering::Equal),
        NumericOp::NotEquals => ordering != Some(Ordering::Equal),
        NumericOp::Greater => ordering == Some(Ordering::Greater),
        NumericOp::Less => ordering == Some(Ordering::Less),
        NumericOp::GreaterEqual => matches!(ordering, Some(Ordering::Greater | Ordering::Equal)),
        NumericOp::LessEqual => matches!(ordering, Some(Ordering::Less | Ordering::Equal)),
        _ => unreachable!("comparison operator required"),
    };
    Value::Boolean(result)
}

fn numeric_error_message(op: &str, left: &Value, right: &Value, error: NumericError) -> String {
    match error {
        NumericError::DivisionByZero => "Division by zero".to_string(),
        NumericError::NotNumeric => {
            let requirement = match op {
                "Add" => "two numbers or two strings",
                "And" | "Or" => "booleans",
                _ => "numbers",
            };
            format!(
                "{} requires {}, found {:?} and {:?}",
                op, requirement, left, right
            )
        }
    }
}

fn apply_float_op(op: NumericOp, left: f64, right: f64) -> Result<Value, NumericError> {
    match op {
        NumericOp::Add => Ok(Value::Float(left + right)),
        NumericOp::Subtract => Ok(Value::Float(left - right)),
        NumericOp::Multiply => Ok(Value::Float(left * right)),
        NumericOp::Divide if right == 0.0 => Err(NumericError::DivisionByZero),
        NumericOp::Divide => Ok(Value::Float(left / right)),
        NumericOp::Equals
        | NumericOp::NotEquals
        | NumericOp::Greater
        | NumericOp::Less
        | NumericOp::GreaterEqual
        | NumericOp::LessEqual => Ok(apply_comparison_op(op, left.partial_cmp(&right))),
    }
}

#[derive(Debug, Clone)]
pub struct ObjectData {
    pub fields: RefCell<IndexMap<Arc<str>, Value>>,
    shape_version: Cell<u64>,
}

/// Monomorphic inline cache: (bytecode, instruction offset) -> object shape and field slot.
type ObjectGetCache = HashMap<(usize, usize), (usize, u64, usize)>;

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Number(a), Value::Float(b)) => {
                compare_i64_f64(*a, *b) == Some(std::cmp::Ordering::Equal)
            }
            (Value::Float(a), Value::Number(b)) => {
                compare_i64_f64(*b, *a) == Some(std::cmp::Ordering::Equal)
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (
                Value::Function {
                    bytecode_offset: a_off,
                    num_params: a_params,
                    upvalues: a_up,
                },
                Value::Function {
                    bytecode_offset: b_off,
                    num_params: b_params,
                    upvalues: b_up,
                },
            ) => a_off == b_off && a_params == b_params && *a_up.borrow() == *b_up.borrow(),
            (
                Value::RegisterFunction { function_index: a, captures: ac },
                Value::RegisterFunction { function_index: b, captures: bc },
            ) => a == b && *ac.borrow() == *bc.borrow(),
            (Value::Array(a), Value::Array(b)) => arrays_equal(a, b),
            (Value::Object(a), Value::Object(b)) => objects_equal(a, b),
            _ => false,
        }
    }
}

fn arrays_equal(a: &Rc<ArrayData>, b: &Rc<ArrayData>) -> bool {
    let len_a = a.len();
    let len_b = b.len();
    if len_a != len_b {
        return false;
    }
    (0..len_a).all(|i| a.get_cloned(i).unwrap() == b.get_cloned(i).unwrap())
}

fn objects_equal(a: &Rc<ObjectData>, b: &Rc<ObjectData>) -> bool {
    let a_fields = a.fields.borrow();
    let b_fields = b.fields.borrow();
    if a_fields.len() != b_fields.len() {
        return false;
    }
    a_fields
        .iter()
        .all(|(k, v)| b_fields.get(k).map(|bv| v == bv).unwrap_or(false))
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{:?}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Function {
                bytecode_offset,
                num_params,
                upvalues,
            } => {
                write!(
                    f,
                    "<function at {:x}({} params, {} upvalues)>",
                    bytecode_offset,
                    num_params,
                    upvalues.borrow().len()
                )
            }
            Value::RegisterFunction { function_index, captures } => {
                write!(
                    f,
                    "<register function {} ({} captures)>",
                    function_index,
                    captures.borrow().len()
                )
            }
            Value::Array(arr) => {
                if arr.mutable {
                    write!(f, "[")?;
                } else {
                    write!(f, "#[")?;
                }
                let len = arr.len();
                for i in 0..len {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arr.get_cloned(i).unwrap_or(Value::Null))?;
                }
                write!(f, "]")
            }
            Value::Object(obj) => {
                let fields = obj.fields.borrow();
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

fn new_object() -> Value {
    Value::Object(Rc::new(ObjectData {
        fields: RefCell::new(IndexMap::new()),
        shape_version: Cell::new(0),
    }))
}

fn value_to_string_key(key: Value) -> Result<Arc<str>, String> {
    match key {
        Value::String(s) => Ok(s.to_arc_str()),
        other => Err(format!("Object key must be a string, found {:?}", other)),
    }
}

fn arc_key(key: &str) -> Arc<str> {
    Arc::from(key)
}

fn object_get(obj: &Value, key: &str) -> Value {
    match obj {
        Value::Null => Value::Null,
        Value::Object(o) => o.fields.borrow().get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn object_get_const_cached(
    cache: &mut ObjectGetCache,
    obj: &Value,
    site: (usize, usize),
    key: &str,
) -> Value {
    match obj {
        Value::Null => Value::Null,
        Value::Object(o) => {
            let ptr = Rc::as_ptr(o) as usize;
            let shape_version = o.shape_version.get();
            if let Some(&(cached_ptr, cached_version, field_idx)) = cache.get(&site) {
                if cached_ptr == ptr && cached_version == shape_version {
                    let fields = o.fields.borrow();
                    if let Some((cached_key, value)) = fields.get_index(field_idx) {
                        if cached_key.as_ref() == key {
                            return value.clone();
                        }
                    }
                }
            }
            let fields = o.fields.borrow();
            if let Some((idx, _, v)) = fields.get_full(key) {
                cache.insert(site, (ptr, shape_version, idx));
                v.clone()
            } else {
                cache.remove(&site);
                Value::Null
            }
        }
        _ => Value::Null,
    }
}

fn object_set(obj: &Value, key: &str, value: Value) -> Result<(), String> {
    match obj {
        Value::Object(o) => {
            let mut fields = o.fields.borrow_mut();
            if !fields.contains_key(key) {
                o.shape_version.set(o.shape_version.get().wrapping_add(1));
            }
            fields.insert(arc_key(key), value);
            Ok(())
        }
        Value::Null => Err("Cannot set property on null".to_string()),
        other => Err(format!("Expected object, found {:?}", other)),
    }
}

fn object_set_arc(obj: &Value, key: Arc<str>, value: Value) -> Result<(), String> {
    match obj {
        Value::Object(o) => {
            let mut fields = o.fields.borrow_mut();
            if !fields.contains_key(key.as_ref()) {
                o.shape_version.set(o.shape_version.get().wrapping_add(1));
            }
            fields.insert(key, value);
            Ok(())
        }
        Value::Null => Err("Cannot set property on null".to_string()),
        other => Err(format!("Expected object, found {:?}", other)),
    }
}

fn object_get_or_create(obj: &Value, key: &str) -> Result<Value, String> {
    match obj {
        Value::Object(o) => {
            let mut fields = o.fields.borrow_mut();
            let needs_new = match fields.get(key) {
                None => true,
                Some(Value::Null) => true,
                Some(Value::Object(_)) => false,
                Some(_) => {
                    return Err(format!(
                        "Cannot traverse through non-object property '{}'",
                        key
                    ));
                }
            };
            if needs_new {
                fields.insert(arc_key(key), new_object());
                o.shape_version.set(o.shape_version.get().wrapping_add(1));
            }
            Ok(fields.get(key).unwrap().clone())
        }
        Value::Null => Err("Cannot set nested property on null".to_string()),
        other => Err(format!("Expected object, found {:?}", other)),
    }
}

fn object_delete(obj: &Value, key: &str) -> Result<(), String> {
    match obj {
        Value::Object(o) => {
            if o.fields.borrow_mut().shift_remove(key).is_some() {
                o.shape_version.set(o.shape_version.get().wrapping_add(1));
            }
            Ok(())
        }
        Value::Null => Ok(()),
        other => Err(format!("Expected object, found {:?}", other)),
    }
}

fn index_to_usize(index: Value) -> Result<usize, String> {
    match index {
        Value::Number(n) if n >= 0 => Ok(n as usize),
        Value::Number(_) => Err("Array index must be non-negative".to_string()),
        other => Err(format!("Array index must be a number, found {:?}", other)),
    }
}

fn parse_input_value(input: &str, has_str: bool, has_int: bool, has_float: bool) -> Option<Value> {
    if has_int {
        if let Ok(value) = input.parse::<i64>() {
            return Some(Value::Number(value));
        }
    }

    if has_float && (!has_int || input.parse::<i64>().is_err()) {
        if let Ok(value) = input.parse::<f64>() {
            if value.is_finite() {
                return Some(Value::Float(value));
            }
        }
    }

    if has_str && input.parse::<i64>().is_err() && input.parse::<f64>().is_err() {
        return Some(Value::String(StringValue::from(input)));
    }

    None
}

fn read_input_line<R: std::io::BufRead>(reader: &mut R) -> Result<String, String> {
    let mut buffer = String::new();
    let bytes_read = reader
        .read_line(&mut buffer)
        .map_err(|e| format!("Input error: {}", e))?;
    if bytes_read == 0 {
        return Err("Input error: end of file reached while waiting for input".to_string());
    }
    Ok(buffer.trim().to_string())
}

fn expect_array(val: Value) -> Result<Rc<ArrayData>, String> {
    match val {
        Value::Array(arr) => Ok(arr),
        other => Err(format!("Expected array, found {:?}", other)),
    }
}

pub struct CallFrame {
    return_addr: usize,
    locals_start: usize, // Index where local variables start in stack
    upvalues: Rc<RefCell<Vec<Value>>>,
}

fn add_delta(value: &mut Value, delta: i64) -> Result<(), String> {
    match value {
        Value::Number(number) => {
            *number += delta;
            Ok(())
        }
        Value::Float(number) => {
            *number += delta as f64;
            Ok(())
        }
        other => Err(numeric_error_message(
            "Add",
            other,
            &Value::Number(delta),
            NumericError::NotNumeric,
        )),
    }
}

fn step_while_less(value: &mut Value, limit: i64, step: i64) -> Result<bool, String> {
    match value {
        Value::Number(number) if *number < limit => {
            *number += step;
            Ok(true)
        }
        Value::Number(_) => Ok(false),
        Value::Float(number) if float_less_than_i64(*number, limit) => {
            *number += step as f64;
            Ok(true)
        }
        Value::Float(_) => Ok(false),
        other => Err(numeric_error_message(
            "Less",
            other,
            &Value::Number(limit),
            NumericError::NotNumeric,
        )),
    }
}

fn less_than_const(value: &Value, limit: i64) -> Result<bool, String> {
    match value {
        Value::Number(number) => Ok(*number < limit),
        Value::Float(number) => Ok(float_less_than_i64(*number, limit)),
        other => Err(numeric_error_message(
            "Less",
            other,
            &Value::Number(limit),
            NumericError::NotNumeric,
        )),
    }
}


pub struct RegisterVM {
    globals: Vec<Value>,
    frames: Vec<Value>,
    call_args: Vec<Value>,
    object_get_cache: HashMap<(usize, String), (u64, Value)>,
}

impl RegisterVM {
    pub fn new() -> Self {
        Self {
            globals: Vec::new(),
            frames: Vec::new(),
            call_args: Vec::new(),
            object_get_cache: HashMap::new(),
        }
    }

    pub fn global(&self, slot: usize) -> Value {
        self.globals.get(slot).cloned().unwrap_or(Value::Null)
    }

    pub fn execute(&mut self, module: &crate::register::SsaModule) -> Result<(), String> {
        self.globals = vec![Value::Null; module.globals.len()];
        self.frames.clear();
        self.call_args.clear();
        self.object_get_cache.clear();
        let empty = Rc::new(RefCell::new(Vec::new()));
        self.execute_function(&module.entry, 0, &[], &empty, &module.functions)?;
        Ok(())
    }

    fn prepare_frame(&mut self, base: usize, function: &crate::register::SsaFunction, args: &[Value]) -> usize {
        let end = base + function.max_registers as usize + function.local_count as usize;
        if self.frames.len() < end {
            self.frames.resize(end, Value::Null);
        }
        for slot in &mut self.frames[base..end] {
            *slot = Value::Null;
        }
        let local_base = base + function.max_registers as usize;
        for (index, value) in args.iter().take(function.params.len()).enumerate() {
            self.frames[local_base + index] = value.clone();
        }
        local_base
    }

    fn global_mut(&mut self, slot: usize) -> &mut Value {
        if slot >= self.globals.len() {
            self.globals.resize(slot + 1, Value::Null);
        }
        &mut self.globals[slot]
    }

    fn execute_function(
        &mut self,
        function: &crate::register::SsaFunction,
        frame_base: usize,
        args: &[Value],
        upvalues: &Rc<RefCell<Vec<Value>>>,
        functions: &[crate::register::SsaFunction],
    ) -> Result<Option<Value>, String> {
        use crate::ast::{BinaryOperator, UnaryOperator};
        use crate::register::{SsaInstruction, SsaTerminator};

        let local_base = self.prepare_frame(frame_base, function, args);
        let mut current = crate::register::BlockId(0);
        loop {
            let block_index = current.0 as usize;
            if block_index >= function.blocks.len() {
                return Err(format!("Register VM entered invalid block {}", block_index));
            }
            let block = &function.blocks[block_index];

            for phi in &block.phis {
                if let Some((value, _)) = phi.incoming.first() {
                    let src = frame_base + value.0 as usize;
                    let dst = frame_base + phi.dst.0 as usize;
                    let copied = self.frames.get(src).cloned().unwrap_or(Value::Null);
                    if dst >= self.frames.len() {
                        self.frames.resize(dst + 1, Value::Null);
                    }
                    self.frames[dst] = copied;
                }
            }

            for instruction in &block.instructions {
                match instruction {
                    SsaInstruction::Const { dst, idx } => {
                        let value = function
                            .constants
                            .get(*idx as usize)
                            .cloned()
                            .ok_or_else(|| format!("Invalid register constant index {}", idx))?;
                        self.frames[frame_base + dst.0 as usize] = value;
                    }
                    SsaInstruction::Null { dst } => {
                        self.frames[frame_base + dst.0 as usize] = Value::Null;
                    }
                    SsaInstruction::LoadGlobal { dst, slot } => {
                        self.frames[frame_base + dst.0 as usize] = self
                            .globals
                            .get(*slot as usize)
                            .cloned()
                            .unwrap_or(Value::Null);
                    }
                    SsaInstruction::StoreGlobal { slot, value } => {
                        let slot = *slot as usize;
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        if slot >= self.globals.len() {
                            self.globals.resize(slot + 1, Value::Null);
                        }
                        self.globals[slot] = stored;
                    }
                    SsaInstruction::LoadLocal { dst, slot } => {
                        let index = local_base + *slot as usize;
                        self.frames[frame_base + dst.0 as usize] =
                            self.frames.get(index).cloned().unwrap_or(Value::Null);
                    }
                    SsaInstruction::StoreLocal { slot, value } => {
                        let index = local_base + *slot as usize;
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        if index >= self.frames.len() {
                            self.frames.resize(index + 1, Value::Null);
                        }
                        self.frames[index] = stored;
                    }
                    SsaInstruction::LoadUpvalue { dst, slot } => {
                        let value = upvalues
                            .borrow()
                            .get(*slot as usize)
                            .cloned()
                            .unwrap_or(Value::Null);
                        self.frames[frame_base + dst.0 as usize] = value;
                    }
                    SsaInstruction::StoreUpvalue { slot, value } => {
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        let mut cells = upvalues.borrow_mut();
                        let slot = *slot as usize;
                        if slot >= cells.len() {
                            cells.resize(slot + 1, Value::Null);
                        }
                        cells[slot] = stored;
                    }
                    SsaInstruction::Move { dst, src } => {
                        self.frames[frame_base + dst.0 as usize] =
                            self.frames[frame_base + src.0 as usize].clone();
                    }
                    SsaInstruction::Unary { dst, op, value } => {
                        let result = if matches!(op, UnaryOperator::Not) {
                            match &self.frames[frame_base + value.0 as usize] {
                                Value::Boolean(value) => Value::Boolean(!value),
                                Value::Null => Value::Boolean(true),
                                Value::Number(value) => Value::Boolean(*value == 0),
                                _ => Value::Boolean(false),
                            }
                        } else {
                            Value::Null
                        };
                        self.frames[frame_base + dst.0 as usize] = result;
                    }
                    SsaInstruction::Binary { dst, op, left, right } => {
                        let left_index = frame_base + left.0 as usize;
                        let right_index = frame_base + right.0 as usize;
                        let dst_index = frame_base + dst.0 as usize;
                        match op {
                            BinaryOperator::And | BinaryOperator::Or => {
                                let result = match op {
                                    BinaryOperator::And => {
                                        matches!(self.frames[left_index], Value::Boolean(true))
                                            && matches!(self.frames[right_index], Value::Boolean(true))
                                    }
                                    _ => {
                                        matches!(self.frames[left_index], Value::Boolean(true))
                                            || matches!(self.frames[right_index], Value::Boolean(true))
                                    }
                                };
                                self.frames[dst_index] = Value::Boolean(result);
                            }
                            BinaryOperator::Equals | BinaryOperator::NotEquals => {
                                let equal = self.frames[left_index] == self.frames[right_index];
                                self.frames[dst_index] = Value::Boolean(match op {
                                    BinaryOperator::Equals => equal,
                                    _ => !equal,
                                });
                            }
                            BinaryOperator::Add
                                if matches!(
                                    (&self.frames[left_index], &self.frames[right_index]),
                                    (Value::String(_), Value::String(_))
                                ) =>
                            {
                                let joined = match (&self.frames[left_index], &self.frames[right_index]) {
                                    (Value::String(left), Value::String(right)) => {
                                        StringValue::concat(left, right)
                                    }
                                    _ => unreachable!(),
                                };
                                self.frames[dst_index] = Value::String(joined);
                            }
                            _ if matches!(
                                (&self.frames[left_index], &self.frames[right_index]),
                                (Value::Number(_), Value::Number(_))
                            ) =>
                            {
                                let result = match (&self.frames[left_index], &self.frames[right_index]) {
                                    (Value::Number(left), Value::Number(right)) => {
                                        apply_register_int_op(*op, *left, *right)
                                    }
                                    _ => unreachable!(),
                                }
                                .map_err(|error| format!("Register VM integer operation failed: {:?}", error))?;
                                self.frames[dst_index] = result;
                            }
                            _ => {
                                let numeric_op = match op {
                                    BinaryOperator::Add => NumericOp::Add,
                                    BinaryOperator::Subtract => NumericOp::Subtract,
                                    BinaryOperator::Multiply => NumericOp::Multiply,
                                    BinaryOperator::Divide => NumericOp::Divide,
                                    BinaryOperator::GreaterThan => NumericOp::Greater,
                                    BinaryOperator::LessThan => NumericOp::Less,
                                    BinaryOperator::GreaterOrEqual => NumericOp::GreaterEqual,
                                    BinaryOperator::LessOrEqual => NumericOp::LessEqual,
                                    BinaryOperator::Equals => NumericOp::Equals,
                                    BinaryOperator::NotEquals => NumericOp::NotEquals,
                                    BinaryOperator::And | BinaryOperator::Or => NumericOp::Add,
                                };
                                let result = apply_numeric_op(
                                    numeric_op,
                                    &self.frames[left_index],
                                    &self.frames[right_index],
                                )
                                .map_err(|error| {
                                    format!(
                                        "Register VM numeric operation failed: {:?} for left={:?}, right={:?}",
                                        error, self.frames[left_index], self.frames[right_index]
                                    )
                                })?;
                                self.frames[dst_index] = result;
                            }
                        }
                    }
                    SsaInstruction::Print { value } => {
                        println!("{}", self.frames[frame_base + value.0 as usize]);
                    }
                    SsaInstruction::Input { dst, prompt, .. } => {
                        println!("{}", self.frames[frame_base + prompt.0 as usize]);
                        let mut line = String::new();
                        std::io::stdin()
                            .read_line(&mut line)
                            .map_err(|error| error.to_string())?;
                        self.frames[frame_base + dst.0 as usize] = if line.is_empty() {
                            Value::Null
                        } else {
                            let line = line.trim().to_string();
                            match line.parse::<i64>() {
                                Ok(value) => Value::Number(value),
                                Err(_) => match line.parse::<f64>() {
                                    Ok(value) => Value::Float(value),
                                    Err(_) => Value::String(StringValue::from(line)),
                                },
                            }
                        };
                    }
                    SsaInstruction::Call {
                        dst,
                        callee,
                        args: arg_values,
                    } => {
                        let callee_value = self.frames[frame_base + callee.0 as usize].clone();
                        let mut call_args = std::mem::take(&mut self.call_args);
                        call_args.clear();
                        call_args.reserve(arg_values.len());
                        for value in arg_values {
                            call_args.push(self.frames[frame_base + value.0 as usize].clone());
                        }
                        let result = match callee_value {
                            Value::RegisterFunction {
                                function_index,
                                captures,
                            } => {
                                let callee_function = functions.get(function_index).ok_or_else(|| {
                                    format!("Invalid register function index {}", function_index)
                                })?;
                                let caller_end = frame_base
                                    + function.max_registers as usize
                                    + function.local_count as usize;
                                let result = self.execute_function(
                                    callee_function,
                                    caller_end,
                                    &call_args,
                                    &captures,
                                    functions,
                                )?;
                                self.frames.truncate(caller_end);
                                result
                            }
                            _ => None,
                        };
                        self.call_args = call_args;
                        self.frames[frame_base + dst.0 as usize] = result.unwrap_or(Value::Null);
                    }
                    SsaInstruction::MakeClosure {
                        dst,
                        function: function_index,
                        captures,
                    } => {
                        let mut resolved = Vec::with_capacity(captures.len());
                        for cap in captures {
                            let value = match cap {
                                crate::register::CaptureKind::Local(slot) => self
                                    .frames
                                    .get(local_base + *slot as usize)
                                    .cloned()
                                    .unwrap_or(Value::Null),
                                crate::register::CaptureKind::Upvalue(slot) => upvalues
                                    .borrow()
                                    .get(*slot as usize)
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            };
                            resolved.push(value);
                        }
                        self.frames[frame_base + dst.0 as usize] = Value::RegisterFunction {
                            function_index: *function_index as usize,
                            captures: Rc::new(RefCell::new(resolved)),
                        };
                    }
                    SsaInstruction::Return { value } => {
                        return Ok(value.map(|value| self.frames[frame_base + value.0 as usize].clone()));
                    }
                    SsaInstruction::BuildArray { dst, elements, mutable } => {
                        let values = elements
                            .iter()
                            .map(|element| self.frames[frame_base + element.0 as usize].clone())
                            .collect();
                        let data = ArrayData {
                            mutable: *mutable,
                            elements: if *mutable {
                                ArrayElements::Mutable(RefCell::new(values))
                            } else {
                                ArrayElements::Immutable(values)
                            },
                        };
                        self.frames[frame_base + dst.0 as usize] = Value::Array(Rc::new(data));
                    }
                    SsaInstruction::BuildObject { dst, fields } => {
                        let object = new_object();
                        if let Value::Object(object_value) = &object {
                            let mut map = object_value.fields.borrow_mut();
                            for (name, value) in fields {
                                map.insert(
                                    Arc::from(name.as_str()),
                                    self.frames[frame_base + value.0 as usize].clone(),
                                );
                            }
                        }
                        self.frames[frame_base + dst.0 as usize] = object;
                    }
                    SsaInstruction::IndexGet { dst, object, index } => {
                        let value = match (
                            &self.frames[frame_base + object.0 as usize],
                            &self.frames[frame_base + index.0 as usize],
                        ) {
                            (Value::Array(array), Value::Number(index)) => {
                                if *index >= 0 {
                                    array.get_cloned(*index as usize).unwrap_or(Value::Null)
                                } else {
                                    Value::Null
                                }
                            }
                            (Value::Object(object), Value::String(index)) => object
                                .fields
                                .borrow()
                                .get(index.to_arc_str().as_ref())
                                .cloned()
                                .unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        self.frames[frame_base + dst.0 as usize] = value;
                    }
                    SsaInstruction::IndexSet { object, index, value } => {
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        match (
                            &self.frames[frame_base + object.0 as usize],
                            &self.frames[frame_base + index.0 as usize],
                        ) {
                            (Value::Array(array), Value::Number(index)) => {
                                if *index >= 0 {
                                    let _ = array.set_at(*index as usize, stored);
                                }
                            }
                            (Value::Object(object), Value::String(index)) => {
                                let mut fields = object.fields.borrow_mut();
                                fields.insert(index.to_arc_str(), stored);
                                object
                                    .shape_version
                                    .set(object.shape_version.get().wrapping_add(1));
                            }
                            _ => {}
                        }
                    }
                    SsaInstruction::PropertyGet { dst, object, member } => {
                        let value = match &self.frames[frame_base + object.0 as usize] {
                            Value::Object(object) => {
                                let object_id = Rc::as_ptr(object) as usize;
                                let shape = object.shape_version.get();
                                if let Some((cached_shape, cached_value)) =
                                    self.object_get_cache.get(&(object_id, member.clone()))
                                {
                                    if *cached_shape == shape {
                                        cached_value.clone()
                                    } else {
                                        let value = object
                                            .fields
                                            .borrow()
                                            .get(member.as_str())
                                            .cloned()
                                            .unwrap_or(Value::Null);
                                        self.object_get_cache.insert(
                                            (object_id, member.clone()),
                                            (shape, value.clone()),
                                        );
                                        value
                                    }
                                } else {
                                    let value = object
                                        .fields
                                        .borrow()
                                        .get(member.as_str())
                                        .cloned()
                                        .unwrap_or(Value::Null);
                                    self.object_get_cache.insert(
                                        (object_id, member.clone()),
                                        (shape, value.clone()),
                                    );
                                    value
                                }
                            }
                            _ => Value::Null,
                        };
                        self.frames[frame_base + dst.0 as usize] = value;
                    }
                    SsaInstruction::PropertySet { object, member, value } => {
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        if let Value::Object(object) = &self.frames[frame_base + object.0 as usize] {
                            let mut fields = object.fields.borrow_mut();
                            fields.insert(Arc::from(member.as_str()), stored);
                            object
                                .shape_version
                                .set(object.shape_version.get().wrapping_add(1));
                        }
                    }
                    SsaInstruction::ArrayLen { dst, object } => {
                        let len = match &self.frames[frame_base + object.0 as usize] {
                            Value::Array(array) => array.len() as i64,
                            _ => 0,
                        };
                        self.frames[frame_base + dst.0 as usize] = Value::Number(len);
                    }
                    SsaInstruction::ArrayPush { object, value } => {
                        let stored = self.frames[frame_base + value.0 as usize].clone();
                        if let Value::Array(array) = &self.frames[frame_base + object.0 as usize] {
                            let _ = array.push(stored);
                        }
                    }
                    SsaInstruction::ArrayPop { dst, object } => {
                        let result = match &self.frames[frame_base + object.0 as usize] {
                            Value::Array(array) => array.pop().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        self.frames[frame_base + dst.0 as usize] = result;
                    }
                    SsaInstruction::ObjectKeys { dst, object } => {
                        let keys = match &self.frames[frame_base + object.0 as usize] {
                            Value::Object(object) => object
                                .fields
                                .borrow()
                                .keys()
                                .map(|key| Value::String(StringValue::from(key.as_ref())))
                                .collect(),
                            _ => Vec::new(),
                        };
                        self.frames[frame_base + dst.0 as usize] = Value::Array(Rc::new(ArrayData {
                            mutable: true,
                            elements: ArrayElements::Mutable(RefCell::new(keys)),
                        }));
                    }
                    SsaInstruction::ObjectDelete { object, member } => {
                        if let Value::Object(object) = &self.frames[frame_base + object.0 as usize] {
                            object.fields.borrow_mut().shift_remove(member.as_str());
                            object
                                .shape_version
                                .set(object.shape_version.get().wrapping_add(1));
                        }
                    }
                    SsaInstruction::ObjectDeleteKey { object, key } => {
                        let key_text = match &self.frames[frame_base + key.0 as usize] {
                            Value::String(text) => Some(text.to_arc_str()),
                            _ => None,
                        };
                        if let (Some(key_text), Value::Object(object)) = (
                            key_text,
                            &self.frames[frame_base + object.0 as usize],
                        ) {
                            object.fields.borrow_mut().shift_remove(key_text.as_ref());
                            object
                                .shape_version
                                .set(object.shape_version.get().wrapping_add(1));
                        }
                    }
                    SsaInstruction::IsArray { dst, value } => {
                        self.frames[frame_base + dst.0 as usize] = Value::Boolean(matches!(
                            self.frames[frame_base + value.0 as usize],
                            Value::Array(_)
                        ));
                    }
                    SsaInstruction::IsObject { dst, value } => {
                        self.frames[frame_base + dst.0 as usize] = Value::Boolean(matches!(
                            self.frames[frame_base + value.0 as usize],
                            Value::Object(_)
                        ));
                    }
                    SsaInstruction::MemberLength { dst, object } => {
                        let len = match &self.frames[frame_base + object.0 as usize] {
                            Value::Array(array) => array.len() as i64,
                            Value::String(value) => value.len() as i64,
                            Value::Object(object) => object.fields.borrow().len() as i64,
                            _ => 0,
                        };
                        self.frames[frame_base + dst.0 as usize] = Value::Number(len);
                    }
                    SsaInstruction::Type { dst, value } => {
                        let type_name = match &self.frames[frame_base + value.0 as usize] {
                            Value::Number(_) | Value::Float(_) => "number",
                            Value::String(_) => "string",
                            Value::Boolean(_) => "boolean",
                            Value::Null => "null",
                            Value::Function { .. } | Value::RegisterFunction { .. } => "function",
                            Value::Array(_) => "array",
                            Value::Object(_) => "object",
                        };
                        self.frames[frame_base + dst.0 as usize] =
                            Value::String(StringValue::from(type_name));
                    }
                    SsaInstruction::Wait { dst, value } => {
                        if let Value::Number(milliseconds) = self.frames[frame_base + value.0 as usize] {
                            std::thread::sleep(std::time::Duration::from_millis(milliseconds.max(0) as u64));
                        }
                        self.frames[frame_base + dst.0 as usize] = Value::Null;
                    }
                    SsaInstruction::AddGlobal { slot, delta } => {
                        add_delta(self.global_mut(*slot as usize), *delta)?;
                    }
                    SsaInstruction::AddLocal { slot, delta } => {
                        let index = local_base + *slot as usize;
                        if index >= self.frames.len() {
                            self.frames.resize(index + 1, Value::Null);
                        }
                        add_delta(&mut self.frames[index], *delta)?;
                    }
                    SsaInstruction::AppendStringGlobal { slot, suffix } => {
                        match self.global_mut(*slot as usize) {
                            Value::String(value) => value.append(suffix),
                            other => {
                                return Err(format!(
                                    "Add requires two numbers or two strings, found {:?} and {:?}",
                                    other, suffix
                                ));
                            }
                        }
                    }
                    SsaInstruction::LoopIncLess {
                        slot,
                        limit,
                        step,
                        local,
                    } => loop {
                        let keep_going = if *local {
                            let index = local_base + *slot as usize;
                            if index >= self.frames.len() {
                                self.frames.resize(index + 1, Value::Null);
                            }
                            step_while_less(&mut self.frames[index], *limit, *step)?
                        } else {
                            step_while_less(self.global_mut(*slot as usize), *limit, *step)?
                        };
                        if !keep_going {
                            break;
                        }
                    },
                }
            }

            match &block.terminator {
                SsaTerminator::Fallthrough => return Ok(None),
                SsaTerminator::Jump(target) => current = *target,
                SsaTerminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition = &self.frames[frame_base + condition.0 as usize];
                    let take_then = match condition {
                        Value::Boolean(value) => *value,
                        Value::Null => false,
                        Value::Number(value) => *value != 0,
                        _ => true,
                    };
                    current = if take_then { *then_block } else { *else_block };
                }
                SsaTerminator::CompareLessConst {
                    slot,
                    limit,
                    local,
                    then_block,
                    else_block,
                } => {
                    let value = if *local {
                        self.frames
                            .get(local_base + *slot as usize)
                            .cloned()
                            .unwrap_or(Value::Null)
                    } else {
                        self.globals.get(*slot as usize).cloned().unwrap_or(Value::Null)
                    };
                    current = if less_than_const(&value, *limit)? {
                        *then_block
                    } else {
                        *else_block
                    };
                }
                SsaTerminator::Return(value) => {
                    return Ok(value.map(|value| self.frames[frame_base + value.0 as usize].clone()));
                }
            }
        }
    }
}

pub struct VM {
    stack: Vec<Value>,
    globals: Vec<Value>,
    call_stack: Vec<CallFrame>,
    object_get_cache: ObjectGetCache,
}

impl VM {
    pub fn new() -> Self {
        VM {
            stack: Vec::with_capacity(256),
            globals: Vec::new(),
            call_stack: Vec::new(),
            object_get_cache: HashMap::new(),
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> Result<Value, String> {
        self.stack
            .pop()
            .ok_or_else(|| "Stack underflow".to_string())
    }

    #[inline(always)]
    fn pop2(&mut self) -> Result<(Value, Value), String> {
        if self.stack.len() < 2 {
            return Err("Stack underflow".to_string());
        }
        let right = self.stack.pop().expect("stack length checked");
        let left = self.stack.pop().expect("stack length checked");
        Ok((left, right))
    }

    #[inline(always)]
    fn pop3(&mut self) -> Result<(Value, Value, Value), String> {
        if self.stack.len() < 3 {
            return Err("Stack underflow".to_string());
        }
        let third = self.stack.pop().expect("stack length checked");
        let second = self.stack.pop().expect("stack length checked");
        let first = self.stack.pop().expect("stack length checked");
        Ok((first, second, third))
    }

    #[inline(always)]
    unsafe fn global_unchecked(&self, idx: usize) -> &Value {
        self.globals.get_unchecked(idx)
    }

    #[inline(always)]
    unsafe fn set_global_unchecked(&mut self, idx: usize, val: Value) {
        *self.globals.get_unchecked_mut(idx) = val;
    }

    pub fn execute(&mut self, bytecode: &Bytecode) -> Result<(), String> {
        self.execute_from(bytecode, 0)?;
        Ok(())
    }

    /// Run bytecode from `start_ip` until end or top-level return. Returns the next IP
    /// (for incremental REPL execution).
    pub fn execute_from(&mut self, bytecode: &Bytecode, start_ip: usize) -> Result<usize, String> {
        let code = &bytecode.code;
        let constants = &bytecode.constants;
        let code_len = code.len();

        if self.globals.len() < bytecode.num_globals {
            self.globals.resize(bytecode.num_globals, Value::Null);
        }
        let globals_len = self.globals.len();

        let mut ip = start_ip;

        while ip < code_len {
            let instruction_ip = ip;
            let opcode = unsafe { *code.get_unchecked(ip) };
            ip += 1;

            match opcode {
                OP_RETURN => {
                    if let Some(frame) = self.call_stack.pop() {
                        // Pop return value from stack
                        let return_value = self.pop()?;
                        // Restore previous frame
                        ip = frame.return_addr;
                        // Push return value back onto stack
                        self.stack.truncate(frame.locals_start);
                        self.stack.push(return_value);
                    } else {
                        // Top-level return, just exit
                        break;
                    }
                }
                OP_LOAD_LOCAL => {
                    let offset = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;

                    if let Some(frame) = self.call_stack.last() {
                        let idx = frame.locals_start + offset;
                        if idx < self.stack.len() {
                            self.stack.push(self.stack[idx].clone());
                        } else {
                            return Err("Local variable out of bounds".to_string());
                        }
                    } else {
                        return Err("Load local outside function context".to_string());
                    }
                }
                OP_STORE_LOCAL => {
                    let offset = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let val = self.pop()?;

                    if let Some(frame) = self.call_stack.last() {
                        let idx = frame.locals_start + offset;
                        if idx < self.stack.len() {
                            self.stack[idx] = val;
                        } else {
                            return Err("Local variable out of bounds".to_string());
                        }
                    } else {
                        return Err("Store local outside function context".to_string());
                    }
                }
                OP_MAKE_CLOSURE => {
                    let func_offset = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;
                    let num_params = Bytecode::read_u32(code, ip);
                    ip += 4;
                    let num_upvalues = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;

                    let mut upvalues = Vec::with_capacity(num_upvalues);
                    let parent_frame = self.call_stack.last();

                    for _ in 0..num_upvalues {
                        let kind = code[ip];
                        ip += 1;
                        let index = Bytecode::read_u32(code, ip) as usize;
                        ip += 4;

                        let value = match kind {
                            CAPTURE_LOCAL => {
                                let frame = parent_frame.ok_or_else(|| {
                                    "Closure capture requires active call frame".to_string()
                                })?;
                                let idx = frame.locals_start + index;
                                if idx >= self.stack.len() {
                                    return Err("Closure local capture out of bounds".to_string());
                                }
                                self.stack[idx].clone()
                            }
                            CAPTURE_UPVALUE => {
                                let frame = parent_frame.ok_or_else(|| {
                                    "Closure capture requires active call frame".to_string()
                                })?;
                                frame.upvalues.borrow().get(index).cloned().ok_or_else(|| {
                                    "Closure upvalue capture out of bounds".to_string()
                                })?
                            }
                            _ => return Err(format!("Unknown capture kind: {}", kind)),
                        };
                        upvalues.push(value);
                    }

                    self.stack.push(Value::Function {
                        bytecode_offset: func_offset,
                        num_params,
                        upvalues: Rc::new(RefCell::new(upvalues)),
                    });
                }
                OP_CALL_FUNCTION => {
                    let num_args = Bytecode::read_u32(code, ip) as u32;
                    ip += 4;

                    let func = self.pop()?;

                    match func {
                        Value::Function {
                            bytecode_offset,
                            num_params,
                            upvalues,
                        } => {
                            if num_params != num_args {
                                return Err(format!(
                                    "Function expects {} arguments, got {}",
                                    num_params, num_args
                                ));
                            }

                            let locals_start = self.stack.len() - num_args as usize;
                            let frame = CallFrame {
                                return_addr: ip,
                                locals_start,
                                upvalues,
                            };

                            self.call_stack.push(frame);
                            ip = bytecode_offset;
                        }
                        _ => return Err("Attempted to call non-function value".to_string()),
                    }
                }
                OP_LOAD_UPVALUE => {
                    let index = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;

                    if let Some(frame) = self.call_stack.last() {
                        let val = frame
                            .upvalues
                            .borrow()
                            .get(index)
                            .cloned()
                            .ok_or_else(|| "Upvalue index out of bounds".to_string())?;
                        self.stack.push(val);
                    } else {
                        return Err("Load upvalue outside function context".to_string());
                    }
                }
                OP_STORE_UPVALUE => {
                    let index = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;
                    let val = self.pop()?;

                    if let Some(frame) = self.call_stack.last_mut() {
                        let mut upvalues = frame.upvalues.borrow_mut();
                        if index < upvalues.len() {
                            upvalues[index] = val;
                        } else {
                            return Err("Upvalue index out of bounds".to_string());
                        }
                    } else {
                        return Err("Store upvalue outside function context".to_string());
                    }
                }
                OP_LOOP_INC_LESS => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let limit = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let loop_start = ip - 13;

                    debug_assert!(idx < globals_len);
                    let global = unsafe { self.global_unchecked(idx) };
                    if let Value::Number(n) = *global {
                        if n < limit {
                            unsafe {
                                self.set_global_unchecked(idx, Value::Number(n + 1));
                            }
                            ip = loop_start;
                        }
                    } else if let Value::Float(n) = *global {
                        if float_less_than_i64(n, limit) {
                            unsafe {
                                self.set_global_unchecked(idx, Value::Float(n + 1.0));
                            }
                            ip = loop_start;
                        }
                    } else {
                        return Err("LoopIncLess expects number".to_string());
                    }
                }
                OP_LOOP_STEP_LESS => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let limit = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let step = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let loop_start = ip - 21;

                    debug_assert!(idx < globals_len);
                    let global = unsafe { self.global_unchecked(idx) };
                    if let Value::Number(n) = *global {
                        if n < limit {
                            unsafe {
                                self.set_global_unchecked(idx, Value::Number(n + step));
                            }
                            ip = loop_start;
                        }
                    } else if let Value::Float(n) = *global {
                        if float_less_than_i64(n, limit) {
                            unsafe {
                                self.set_global_unchecked(idx, Value::Float(n + step as f64));
                            }
                            ip = loop_start;
                        }
                    } else {
                        return Err("LoopStepLess expects number".to_string());
                    }
                }
                OP_LOOP_INC_LESS_LOCAL => {
                    let offset = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let limit = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let loop_start = ip - 13;
                    let frame = self
                        .call_stack
                        .last()
                        .ok_or_else(|| "Local loop outside function context".to_string())?;
                    let idx = frame.locals_start + offset;
                    match self.stack.get(idx) {
                        Some(Value::Number(n)) if *n < limit => {
                            self.stack[idx] = Value::Number(*n + 1);
                            ip = loop_start;
                        }
                        Some(Value::Number(_)) => {}
                        Some(Value::Float(n)) if float_less_than_i64(*n, limit) => {
                            self.stack[idx] = Value::Float(*n + 1.0);
                            ip = loop_start;
                        }
                        Some(Value::Float(_)) => {}
                        Some(value) => {
                            return Err(numeric_error_message(
                                "Less",
                                value,
                                &Value::Number(limit),
                                NumericError::NotNumeric,
                            ));
                        }
                        None => return Err("Local variable out of bounds".to_string()),
                    }
                }
                OP_LOOP_STEP_LESS_LOCAL => {
                    let offset = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let limit = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let step = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let loop_start = ip - 21;
                    let frame = self
                        .call_stack
                        .last()
                        .ok_or_else(|| "Local loop outside function context".to_string())?;
                    let idx = frame.locals_start + offset;
                    match self.stack.get(idx) {
                        Some(Value::Number(n)) if *n < limit => {
                            self.stack[idx] = Value::Number(*n + step);
                            ip = loop_start;
                        }
                        Some(Value::Number(_)) => {}
                        Some(Value::Float(n)) if float_less_than_i64(*n, limit) => {
                            self.stack[idx] = Value::Float(*n + step as f64);
                            ip = loop_start;
                        }
                        Some(Value::Float(_)) => {}
                        Some(value) => {
                            return Err(numeric_error_message(
                                "Less",
                                value,
                                &Value::Number(limit),
                                NumericError::NotNumeric,
                            ));
                        }
                        None => return Err("Local variable out of bounds".to_string()),
                    }
                }
                OP_INC_LOCAL | OP_ADD_LOCAL => {
                    let offset = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let delta = if opcode == OP_INC_LOCAL {
                        1
                    } else {
                        let delta = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                        ip += 8;
                        delta
                    };
                    let frame = self
                        .call_stack
                        .last()
                        .ok_or_else(|| "Local arithmetic outside function context".to_string())?;
                    let idx = frame.locals_start + offset;
                    let Some(value) = self.stack.get(idx) else {
                        return Err("Local variable out of bounds".to_string());
                    };
                    match value {
                        Value::Number(n) => self.stack[idx] = Value::Number(*n + delta),
                        Value::Float(n) => self.stack[idx] = Value::Float(*n + delta as f64),
                        _ => {
                            return Err(numeric_error_message(
                                "Add",
                                value,
                                &Value::Number(delta),
                                NumericError::NotNumeric,
                            ));
                        }
                    }
                }
                OP_APPEND_STRING_GLOBAL => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let suffix_idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let suffix_value = match constants.get(suffix_idx) {
                        Some(value @ Value::String(_)) => value.clone(),
                        _ => {
                            return Err("String append suffix must be a string constant".to_string())
                        }
                    };
                    let Value::String(suffix) = &suffix_value else {
                        unreachable!()
                    };
                    match self.globals.get_mut(idx) {
                        Some(Value::String(value)) => value.append(suffix.as_str()),
                        Some(value) => {
                            return Err(format!(
                                "Add requires two numbers or two strings, found {:?} and {:?}",
                                value, suffix_value
                            ));
                        }
                        None => return Err("Global index out of bounds".to_string()),
                    }
                }
                OP_LESS_CONST_JUMP_IF_FALSE => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let limit = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;
                    let target = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;

                    debug_assert!(idx < globals_len);
                    let global = unsafe { self.global_unchecked(idx) };
                    if let Value::Number(n) = *global {
                        if n >= limit {
                            ip = target;
                        }
                    } else if let Value::Float(n) = *global {
                        if !float_less_than_i64(n, limit) {
                            ip = target;
                        }
                    } else {
                        return Err("LessConstJumpIfFalse expects number".to_string());
                    }
                }
                OP_ADD_GLOBAL => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let rhs = unsafe { Bytecode::read_i64_unchecked(code, ip) };
                    ip += 8;

                    debug_assert!(idx < globals_len);
                    let global = unsafe { self.global_unchecked(idx) };
                    if let Value::Number(n) = *global {
                        unsafe {
                            self.set_global_unchecked(idx, Value::Number(n + rhs));
                        }
                    } else if let Value::Float(n) = *global {
                        unsafe {
                            self.set_global_unchecked(idx, Value::Float(n + rhs as f64));
                        }
                    } else {
                        return Err("AddGlobal expects number".to_string());
                    }
                }
                OP_INC_GLOBAL => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;

                    debug_assert!(idx < globals_len);
                    let global = unsafe { self.global_unchecked(idx) };
                    if let Value::Number(n) = *global {
                        unsafe {
                            self.set_global_unchecked(idx, Value::Number(n + 1));
                        }
                    } else if let Value::Float(n) = *global {
                        unsafe {
                            self.set_global_unchecked(idx, Value::Float(n + 1.0));
                        }
                    } else {
                        return Err("IncGlobal expects number".to_string());
                    }
                }
                OP_LOAD_GLOBAL => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    debug_assert!(idx < globals_len);
                    self.stack
                        .push(unsafe { self.global_unchecked(idx).clone() });
                }
                OP_STORE_GLOBAL => {
                    let idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let val = self.pop()?;
                    debug_assert!(idx < globals_len);
                    unsafe {
                        self.set_global_unchecked(idx, val);
                    }
                }
                OP_JUMP => {
                    ip = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                }
                OP_JUMP_IF_FALSE => {
                    let target = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let cond = self.pop()?;
                    match cond {
                        Value::Boolean(b) => {
                            if !b {
                                ip = target;
                            }
                        }
                        _ => return Err("JumpIfFalse expects boolean".to_string()),
                    }
                }
                OP_CONSTANT => {
                    let const_idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    self.stack.push(constants[const_idx].clone());
                }
                OP_NULL => {
                    self.stack.push(Value::Null);
                }
                OP_ADD => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::String(l), Value::String(r)) => {
                            self.stack.push(Value::String(StringValue::concat(&l, &r)));
                        }
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Number(left + right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Add, &left, &right).map_err(
                                |error| numeric_error_message("Add", &left, &right, error),
                            )?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_SUBTRACT => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Number(left - right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Subtract, &left, &right)
                                .map_err(|error| {
                                    numeric_error_message("Subtract", &left, &right, error)
                                })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_MULTIPLY => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Number(left * right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Multiply, &left, &right)
                                .map_err(|error| {
                                    numeric_error_message("Multiply", &left, &right, error)
                                })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_DIVIDE => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(_), Value::Number(0)) => {
                            return Err("Division by zero".to_string());
                        }
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Number(left / right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Divide, &left, &right)
                                .map_err(|error| {
                                    numeric_error_message("Divide", &left, &right, error)
                                })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_EQUALS => {
                    let (left, right) = self.pop2()?;
                    self.stack.push(Value::Boolean(left == right));
                }
                OP_NOT_EQUALS => {
                    let (left, right) = self.pop2()?;
                    self.stack.push(Value::Boolean(left != right));
                }
                OP_GREATER => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Boolean(left > right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Greater, &left, &right)
                                .map_err(|error| {
                                    numeric_error_message("Greater", &left, &right, error)
                                })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_LESS => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Boolean(left < right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::Less, &left, &right).map_err(
                                |error| numeric_error_message("Less", &left, &right, error),
                            )?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_GREATER_EQUAL => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Boolean(left >= right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::GreaterEqual, &left, &right)
                                .map_err(|error| {
                                numeric_error_message("GreaterEqual", &left, &right, error)
                            })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_LESS_EQUAL => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Number(left), Value::Number(right)) => {
                            self.stack.push(Value::Boolean(left <= right));
                        }
                        (left, right) => {
                            let result = apply_numeric_op(NumericOp::LessEqual, &left, &right)
                                .map_err(|error| {
                                    numeric_error_message("LessEqual", &left, &right, error)
                                })?;
                            self.stack.push(result);
                        }
                    }
                }
                OP_AND => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Boolean(l), Value::Boolean(r)) => {
                            self.stack.push(Value::Boolean(l && r));
                        }
                        (l, r) => {
                            return Err(format!("And requires booleans, found {:?} and {:?}", l, r))
                        }
                    }
                }
                OP_OR => {
                    let (left, right) = self.pop2()?;
                    match (left, right) {
                        (Value::Boolean(l), Value::Boolean(r)) => {
                            self.stack.push(Value::Boolean(l || r));
                        }
                        (l, r) => {
                            return Err(format!("Or requires booleans, found {:?} and {:?}", l, r))
                        }
                    }
                }
                OP_NOT => {
                    let operand = self.pop()?;
                    match operand {
                        Value::Boolean(b) => self.stack.push(Value::Boolean(!b)),
                        other => return Err(format!("Not requires boolean, found {:?}", other)),
                    }
                }
                OP_PRINT => {
                    let val = self.pop()?;
                    println!("{}", val);
                }
                OP_POP => {
                    self.pop()?;
                }
                OP_BUILD_ARRAY => {
                    let count = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let mutable = code[ip] != 0;
                    ip += 1;

                    let mut elements = Vec::with_capacity(count);
                    for i in 0..count {
                        let slot = self.stack.len() - count + i;
                        elements.push(self.stack[slot].clone());
                    }
                    self.stack.truncate(self.stack.len() - count);

                    let storage = if mutable {
                        ArrayElements::Mutable(RefCell::new(elements))
                    } else {
                        ArrayElements::Immutable(elements)
                    };
                    self.stack.push(Value::Array(Rc::new(ArrayData {
                        mutable,
                        elements: storage,
                    })));
                }
                OP_INDEX_GET => {
                    let (container, index_val) = self.pop2()?;
                    match container {
                        Value::Array(arr) => {
                            let idx = index_to_usize(index_val)?;
                            let val = arr
                                .get_cloned(idx)
                                .ok_or_else(|| format!("Array index {} out of bounds", idx))?;
                            self.stack.push(val);
                        }
                        Value::Object(_) | Value::Null => {
                            let key = value_to_string_key(index_val)?;
                            self.stack.push(object_get(&container, key.as_ref()));
                        }
                        other => {
                            return Err(format!("Cannot index into {:?}", other));
                        }
                    }
                }
                OP_INDEX_SET => {
                    let (container, index_val, value) = self.pop3()?;
                    match container {
                        Value::Array(arr) => {
                            let idx = index_to_usize(index_val)?;
                            arr.set_at(idx, value)?;
                        }
                        Value::Object(_) => {
                            let key = value_to_string_key(index_val)?;
                            object_set_arc(&container, key, value)?;
                        }
                        Value::Null => {
                            return Err("Cannot set property on null".to_string());
                        }
                        other => {
                            return Err(format!("Cannot index into {:?}", other));
                        }
                    }
                }
                OP_ARRAY_LEN => {
                    let array_val = self.pop()?;
                    let arr = expect_array(array_val)?;
                    self.stack.push(Value::Number(arr.len() as i64));
                }
                OP_ARRAY_PUSH => {
                    let (array_val, value) = self.pop2()?;
                    let arr = expect_array(array_val)?;
                    arr.push(value)?;
                }
                OP_ARRAY_POP => {
                    let array_val = self.pop()?;
                    let arr = expect_array(array_val)?;
                    self.stack.push(arr.pop()?);
                }
                OP_BUILD_OBJECT => {
                    let count = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;
                    let mut map = IndexMap::new();
                    let mut keys = Vec::with_capacity(count);
                    for _ in 0..count {
                        let key_idx = Bytecode::read_u32(code, ip) as usize;
                        ip += 4;
                        keys.push(key_idx);
                    }
                    let mut values = Vec::with_capacity(count);
                    for _ in 0..count {
                        values.push(self.pop()?);
                    }
                    values.reverse();
                    for (i, key_idx) in keys.iter().enumerate() {
                        let key = match &constants[*key_idx] {
                            Value::String(s) => s.to_arc_str(),
                            _ => return Err("Object key must be a string constant".to_string()),
                        };
                        map.insert(key, values[i].clone());
                    }
                    self.stack.push(Value::Object(Rc::new(ObjectData {
                        fields: RefCell::new(map),
                        shape_version: Cell::new(0),
                    })));
                }
                OP_OBJECT_GET_CONST => {
                    let key_idx = unsafe { Bytecode::read_u32_unchecked(code, ip) } as u32;
                    ip += 4;
                    let key = match &constants[key_idx as usize] {
                        Value::String(s) => s.as_str(),
                        _ => return Err("Object key must be a string constant".to_string()),
                    };
                    let obj = self.pop()?;
                    self.stack.push(object_get_const_cached(
                        &mut self.object_get_cache,
                        &obj,
                        (code.as_ptr() as usize, instruction_ip),
                        key,
                    ));
                }
                OP_OBJECT_GET => {
                    let (obj, key_val) = self.pop2()?;
                    let key = value_to_string_key(key_val)?;
                    self.stack.push(object_get(&obj, &key));
                }
                OP_OBJECT_SET => {
                    let (obj, key_val, value) = self.pop3()?;
                    let key = value_to_string_key(key_val)?;
                    object_set(&obj, &key, value)?;
                }
                OP_OBJECT_DELETE => {
                    let (obj, key_val) = self.pop2()?;
                    let key = value_to_string_key(key_val)?;
                    object_delete(&obj, &key)?;
                }
                OP_OBJECT_GET_OR_CREATE_CONST => {
                    let key_idx = Bytecode::read_u32(code, ip) as usize;
                    ip += 4;
                    let key = match &constants[key_idx] {
                        Value::String(s) => s.as_str(),
                        _ => return Err("Object key must be a string constant".to_string()),
                    };
                    let obj = self.pop()?;
                    match obj {
                        Value::Object(_) => {
                            self.stack.push(object_get_or_create(&obj, key)?);
                        }
                        Value::Null => {
                            return Err("Cannot set nested property on null".to_string());
                        }
                        other => {
                            return Err(format!("Expected object, found {:?}", other));
                        }
                    }
                }
                OP_IS_ARRAY => {
                    let val = self.pop()?;
                    self.stack
                        .push(Value::Boolean(matches!(val, Value::Array(_))));
                }
                OP_IS_OBJECT => {
                    let val = self.pop()?;
                    self.stack
                        .push(Value::Boolean(matches!(val, Value::Object(_))));
                }
                OP_MEMBER_LENGTH => {
                    let val = self.pop()?;
                    match val {
                        Value::Array(arr) => {
                            self.stack.push(Value::Number(arr.len() as i64));
                        }
                        _ => self.stack.push(Value::Null),
                    }
                }
                OP_OBJECT_GET_OR_CREATE => {
                    let key_val = self.pop()?;
                    let obj = self.pop()?;
                    let key = value_to_string_key(key_val)?;
                    match obj {
                        Value::Object(_) => {
                            self.stack.push(object_get_or_create(&obj, &key)?);
                        }
                        Value::Null => {
                            return Err("Cannot set nested property on null".to_string());
                        }
                        other => {
                            return Err(format!("Expected object, found {:?}", other));
                        }
                    }
                }
                OP_OBJECT_KEYS => {
                    let obj = self.pop()?;
                    match obj {
                        Value::Object(o) => {
                            let fields = o.fields.borrow();
                            let keys: Vec<Value> = fields
                                .keys()
                                .map(|k| Value::String(StringValue::from(Arc::clone(k))))
                                .collect();
                            self.stack.push(Value::Array(Rc::new(ArrayData {
                                mutable: true,
                                elements: ArrayElements::Mutable(RefCell::new(keys)),
                            })));
                        }
                        Value::Null => {
                            self.stack.push(Value::Array(Rc::new(ArrayData {
                                mutable: true,
                                elements: ArrayElements::Mutable(RefCell::new(Vec::new())),
                            })));
                        }
                        other => {
                            return Err(format!("Expected object for keys(), found {:?}", other));
                        }
                    }
                }
                OP_FOR_IN_ARRAY_HEADER => {
                    let arr_g = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let i_g = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let exit = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;

                    let i_val = unsafe { self.global_unchecked(i_g) };
                    let i = match *i_val {
                        Value::Number(n) if n >= 0 => n as usize,
                        _ => return Err("for-in index must be a non-negative number".to_string()),
                    };
                    let arr_val = unsafe { self.global_unchecked(arr_g).clone() };
                    let arr = expect_array(arr_val)?;
                    match arr.get_cloned(i) {
                        Some(item) => self.stack.push(item),
                        None => ip = exit,
                    }
                }
                OP_FOR_IN_ARRAY_NEXT => {
                    let i_g = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;
                    let loop_start = unsafe { Bytecode::read_u32_unchecked(code, ip) } as usize;
                    ip += 4;

                    let global = unsafe { self.global_unchecked(i_g) };
                    if let Value::Number(n) = *global {
                        unsafe {
                            self.set_global_unchecked(i_g, Value::Number(n + 1));
                        }
                    } else {
                        return Err("for-in index must be a number".to_string());
                    }
                    ip = loop_start;
                }
                OP_INPUT => {
                    // Read type mask
                    let type_mask = code[ip];
                    ip += 1;

                    // Pop prompt from stack
                    let prompt = self.pop()?;
                    let prompt_str = match prompt {
                        Value::String(s) => s.to_string(),
                        _ => return Err("Input prompt must be a string".to_string()),
                    };

                    // Print prompt
                    print!("{}", prompt_str);
                    use std::io::{self, Write};
                    let _ = io::stdout().flush();

                    // Read input with retry loop
                    let has_str = (type_mask & 0x01) != 0;
                    let has_int = (type_mask & 0x02) != 0;
                    let has_float = (type_mask & 0x04) != 0;
                    let mut stdin = std::io::stdin().lock();

                    let value = loop {
                        let trimmed = read_input_line(&mut stdin)?;

                        if let Some(value) =
                            parse_input_value(&trimmed, has_str, has_int, has_float)
                        {
                            break value;
                        }

                        // Invalid input, print error and retry
                        eprint!("Invalid input. Expected: ");
                        let mut expected = Vec::new();
                        if has_str {
                            expected.push("string");
                        }
                        if has_int {
                            expected.push("integer");
                        }
                        if has_float {
                            expected.push("float");
                        }
                        eprintln!("{}", expected.join(" or "));
                        eprint!("Try again: ");
                        let _ = io::stderr().flush();
                    };

                    self.stack.push(value);
                }
                OP_TYPE => {
                    let val = self.pop()?;
                    let type_name = match &val {
                        Value::Number(_) | Value::Float(_) => "number",
                        Value::String(_) => "string",
                        Value::Boolean(_) => "boolean",
                        Value::Null => "null",
                        Value::Function { .. } | Value::RegisterFunction { .. } => "function",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    };
                    self.stack.push(Value::String(StringValue::from(type_name)));
                }
                OP_WAIT => {
                    let val = self.pop()?;
                    let duration = match val {
                        Value::Number(n) => {
                            if n < 0 {
                                return Err(
                                    "wait() requires a non-negative number of milliseconds"
                                        .to_string(),
                                );
                            }
                            std::time::Duration::from_millis(n as u64)
                        }
                        Value::Float(n) if n.is_finite() && n >= 0.0 => {
                            std::time::Duration::try_from_secs_f64(n / 1000.0)
                                .map_err(|_| "wait() duration is too large".to_string())?
                        }
                        Value::Float(_) => {
                            return Err(
                                "wait() requires a finite, non-negative number of milliseconds"
                                    .to_string(),
                            );
                        }
                        _ => return Err("wait() requires a number (milliseconds)".to_string()),
                    };
                    std::thread::sleep(duration);
                }
                _ => return Err(format!("Unknown opcode: {}", opcode)),
            }
        }

        Ok(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn cached_object_get_survives_removing_an_earlier_field() {
        let object = new_object();
        object_set(&object, "a", Value::Number(1)).unwrap();
        object_set(&object, "b", Value::Number(2)).unwrap();
        object_set(&object, "c", Value::Number(3)).unwrap();

        let mut cache = ObjectGetCache::new();
        assert_eq!(
            object_get_const_cached(&mut cache, &object, (0, 7), "b"),
            Value::Number(2)
        );
        object_delete(&object, "a").unwrap();

        assert_eq!(
            object_get_const_cached(&mut cache, &object, (0, 7), "b"),
            Value::Number(2)
        );
    }

    #[test]
    fn property_cache_tracks_shape_and_instruction_site() {
        let object = new_object();
        object_set(&object, "a", Value::Number(1)).unwrap();
        let mut cache = ObjectGetCache::new();
        assert_eq!(
            object_get_const_cached(&mut cache, &object, (10, 3), "a"),
            Value::Number(1)
        );

        object_set(&object, "b", Value::Number(2)).unwrap();
        assert_eq!(
            object_get_const_cached(&mut cache, &object, (10, 3), "a"),
            Value::Number(1)
        );
        object_delete(&object, "a").unwrap();
        assert_eq!(
            object_get_const_cached(&mut cache, &object, (10, 3), "a"),
            Value::Null
        );
        assert_eq!(
            object_get_const_cached(&mut cache, &object, (10, 4), "b"),
            Value::Number(2)
        );
    }

    #[test]
    fn string_append_fast_path_preserves_shared_values() {
        let original = StringValue::from("before");
        let mut vm = VM::new();
        vm.globals = vec![Value::String(original.clone()), Value::String(original)];
        let mut bytecode = Bytecode::new();
        bytecode.num_globals = 2;
        bytecode
            .constants
            .push(Value::String(StringValue::from(" after")));
        bytecode.emit_byte(OP_APPEND_STRING_GLOBAL);
        bytecode.emit_u32(0);
        bytecode.emit_u32(0);

        vm.execute(&bytecode).expect("append");
        assert_eq!(
            vm.globals[0],
            Value::String(StringValue::from("before after"))
        );
        assert_eq!(vm.globals[1], Value::String(StringValue::from("before")));
    }

    #[test]
    fn float_display_keeps_decimal_notation_for_integral_values() {
        assert_eq!(Value::Float(2.5).to_string(), "2.5");
        assert_eq!(Value::Float(2.0).to_string(), "2.0");
    }

    #[test]
    fn array_indices_remain_integers() {
        assert_eq!(index_to_usize(Value::Number(2)), Ok(2));
        assert!(index_to_usize(Value::Float(2.0)).is_err());
    }

    #[test]
    fn mixed_numeric_comparisons_preserve_integer_precision() {
        let large_integer = Value::Number(9_007_199_254_740_993);
        let rounded_float = Value::Float(9_007_199_254_740_992.0);
        assert_ne!(large_integer, rounded_float);
        assert_eq!(
            apply_numeric_op(NumericOp::Greater, &large_integer, &rounded_float),
            Ok(Value::Boolean(true))
        );
        assert_eq!(
            apply_numeric_op(NumericOp::Divide, &Value::Float(1.0), &Value::Number(0)),
            Err(NumericError::DivisionByZero)
        );
        assert_eq!(
            apply_numeric_op(
                NumericOp::Equals,
                &Value::Number(i64::MIN),
                &Value::Float(-9_223_372_036_854_775_808.0)
            ),
            Ok(Value::Boolean(true))
        );
        assert_eq!(
            apply_numeric_op(
                NumericOp::Less,
                &Value::Number(i64::MAX),
                &Value::Float(9_223_372_036_854_775_808.0)
            ),
            Ok(Value::Boolean(true))
        );
        assert!(float_less_than_i64(
            9_007_199_254_740_992.0,
            9_007_199_254_740_993
        ));
        assert!(!float_less_than_i64(f64::NAN, 10));
    }

    #[test]
    fn float_input_parses_fractional_and_integral_values_as_floats() {
        assert_eq!(
            parse_input_value("2.75", false, false, true),
            Some(Value::Float(2.75))
        );
        assert_eq!(
            parse_input_value("2", false, false, true),
            Some(Value::Float(2.0))
        );
        assert_eq!(
            parse_input_value("2", false, true, true),
            Some(Value::Number(2))
        );
        assert_eq!(
            parse_input_value("word", true, false, true),
            Some(Value::String(StringValue::from("word")))
        );
        assert_eq!(parse_input_value("1e999", false, false, true), None);
    }

    #[test]
    fn input_reader_returns_error_at_eof() {
        let mut empty = std::io::Cursor::new(Vec::<u8>::new());
        let error = read_input_line(&mut empty).unwrap_err();
        assert_eq!(
            error,
            "Input error: end of file reached while waiting for input"
        );
    }

    #[test]
    fn input_reader_reads_one_line_without_repeated_eof() {
        let mut input = std::io::Cursor::new(b"42\n".to_vec());
        assert_eq!(read_input_line(&mut input).unwrap(), "42");
        assert!(read_input_line(&mut input).is_err());
    }

    #[test]
    fn function_returns_release_arguments_and_locals_from_stack() {
        let source = "fn f(a) {\n set b = a + 1\n return b\n}\nset i = 0\nwhile (i < 100) {\n f(i)\n set i = i + 1\n}";
        let tokens = Lexer::new(source).tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().expect("parse");
        let mut compiler = Compiler::new();
        compiler.compile_module(&statements).expect("compile");
        let bytecode = compiler.finish_bytecode();

        let mut vm = VM::new();
        vm.execute(&bytecode).expect("execute");

        assert!(vm.stack.is_empty());
    }
}
