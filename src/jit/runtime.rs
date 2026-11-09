use super::value::{LlvmValue, TAG_BOOL, TAG_FLOAT, TAG_HEAP, TAG_INT, TAG_NULL};
use crate::ast::BinaryOperator;
use crate::vm::{
    apply_numeric_op, new_object, ArrayData, ArrayElements, NumericOp, StringValue, Value,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum HeapIdentity {
    Array(usize),
    Object(usize),
    Closure(usize, usize),
}

pub type JitFn =
    extern "C" fn(*mut KriaContext, *const KriaRuntime, u64, *const LlvmValue, u32) -> LlvmValue;

#[repr(C)]
pub struct KriaRuntime {
    pub print_fn: extern "C" fn(*mut KriaContext, LlvmValue),
    pub input_fn: extern "C" fn(*mut KriaContext, LlvmValue, u8) -> LlvmValue,
    pub binop_fn: extern "C" fn(*mut KriaContext, u32, LlvmValue, LlvmValue) -> LlvmValue,
    pub unary_fn: extern "C" fn(*mut KriaContext, u32, LlvmValue) -> LlvmValue,
    pub build_array_fn: extern "C" fn(*mut KriaContext, *const LlvmValue, u32, u32) -> LlvmValue,
    pub build_object_fn: extern "C" fn(
        *mut KriaContext,
        *const *const u8,
        *const u32,
        *const LlvmValue,
        u32,
    ) -> LlvmValue,
    pub index_get_fn: extern "C" fn(*mut KriaContext, LlvmValue, LlvmValue) -> LlvmValue,
    pub index_set_fn: extern "C" fn(*mut KriaContext, LlvmValue, LlvmValue, *const LlvmValue),
    pub prop_get_fn: extern "C" fn(*mut KriaContext, LlvmValue, u32, *const u8, u32) -> LlvmValue,
    pub prop_set_fn: extern "C" fn(*mut KriaContext, LlvmValue, *const u8, u32, *const LlvmValue),
    pub array_len_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub array_push_fn: extern "C" fn(*mut KriaContext, LlvmValue, *const LlvmValue),
    pub array_pop_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub object_keys_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub object_del_fn: extern "C" fn(*mut KriaContext, LlvmValue, *const u8, u32),
    pub object_del_key_fn: extern "C" fn(*mut KriaContext, LlvmValue, LlvmValue),
    pub is_array_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub is_object_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub member_len_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub type_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub wait_fn: extern "C" fn(*mut KriaContext, LlvmValue) -> LlvmValue,
    pub append_str_global_fn: extern "C" fn(*mut KriaContext, u32, *const u8, u32),
    pub make_closure_fn:
        extern "C" fn(*mut KriaContext, u32, *const LlvmValue, *const u8, u32, u64) -> LlvmValue,
    pub call_closure_fn: extern "C" fn(
        *mut KriaContext,
        *const KriaRuntime,
        LlvmValue,
        *const LlvmValue,
        u32,
    ) -> LlvmValue,
    pub load_upvalue_fn: extern "C" fn(*mut KriaContext, u64, u32) -> LlvmValue,
    pub store_upvalue_fn: extern "C" fn(*mut KriaContext, u64, u32, *const LlvmValue),
    pub set_error_fn: extern "C" fn(*mut KriaContext, *const u8, u32),
    pub has_error_fn: extern "C" fn(*mut KriaContext) -> u32,
}

#[repr(C)]
pub struct KriaContext {
    pub globals_ptr: *mut LlvmValue,
    pub globals_len: usize,
    pub constants_ptr: *const LlvmValue,
    pub globals: Vec<LlvmValue>,
    pub heap_objects: Vec<Value>,
    heap_identities: HashMap<HeapIdentity, usize>,
    property_cache: HashMap<(u32, usize), (u64, usize)>,
    pub upvalue_cells: Vec<Rc<RefCell<Vec<Value>>>>,
    pub function_ptrs: Vec<JitFn>,
    pub constants: Vec<LlvmValue>,
    pub error: Option<String>,
    pub num_constants: usize,
}

impl KriaContext {
    pub fn new(num_globals: usize, constants: &[Value]) -> Self {
        let globals = vec![LlvmValue::NULL; num_globals];
        let mut ctx = Self {
            globals_ptr: std::ptr::null_mut(),
            globals_len: num_globals,
            constants_ptr: std::ptr::null(),
            globals,
            heap_objects: Vec::new(),
            heap_identities: HashMap::new(),
            property_cache: HashMap::new(),
            upvalue_cells: Vec::new(),
            function_ptrs: Vec::new(),
            constants: Vec::with_capacity(constants.len()),
            error: None,
            num_constants: constants.len(),
        };
        for c in constants {
            let llvm_val = ctx.from_value(c.clone());
            ctx.constants.push(llvm_val);
        }
        ctx.sync_abi_pointers();
        ctx
    }

    pub fn set_function_ptrs(&mut self, function_ptrs: Vec<JitFn>) {
        self.function_ptrs = function_ptrs;
        self.sync_abi_pointers();
    }

    fn sync_abi_pointers(&mut self) {
        self.globals_ptr = self.globals.as_mut_ptr();
        self.constants_ptr = self.constants.as_ptr();
    }

    pub fn to_value(&self, val: LlvmValue) -> Value {
        match val.tag {
            TAG_NULL => Value::Null,
            TAG_INT => Value::Number(val.payload as i64),
            TAG_FLOAT => Value::Float(f64::from_bits(val.payload)),
            TAG_BOOL => Value::Boolean(val.payload != 0),
            TAG_HEAP => {
                let idx = val.payload as usize;
                if idx < self.heap_objects.len() {
                    self.heap_objects[idx].clone()
                } else {
                    Value::Null
                }
            }
            _ => Value::Null,
        }
    }

    pub fn from_value(&mut self, val: Value) -> LlvmValue {
        match val {
            Value::Null => LlvmValue::NULL,
            Value::Number(n) => LlvmValue::int(n),
            Value::Float(f) => LlvmValue::float(f),
            Value::Boolean(b) => LlvmValue::bool(b),
            heap_val => {
                let identity = match &heap_val {
                    Value::Array(array) => Some(HeapIdentity::Array(Rc::as_ptr(array) as usize)),
                    Value::Object(object) => {
                        Some(HeapIdentity::Object(Rc::as_ptr(object) as usize))
                    }
                    Value::RegisterFunction {
                        function_index,
                        captures,
                    } => Some(HeapIdentity::Closure(
                        Rc::as_ptr(captures) as usize,
                        *function_index,
                    )),
                    #[cfg(test)]
                    Value::Function { .. } => None,
                    Value::String(_) => None,
                    Value::Null | Value::Number(_) | Value::Float(_) | Value::Boolean(_) => {
                        unreachable!("primitive values handled above")
                    }
                };
                if let Some(identity) = identity {
                    if let Some(&idx) = self.heap_identities.get(&identity) {
                        return LlvmValue::heap(idx);
                    }
                }
                let idx = self.heap_objects.len();
                self.heap_objects.push(heap_val);
                if let Some(identity) = identity {
                    self.heap_identities.insert(identity, idx);
                }
                LlvmValue::heap(idx)
            }
        }
    }

    pub fn set_error(&mut self, msg: String) {
        if self.error.is_none() {
            self.error = Some(msg);
        }
    }
}

pub static RUNTIME_TABLE: KriaRuntime = KriaRuntime {
    print_fn: rt_print,
    input_fn: rt_input,
    binop_fn: rt_binop,
    unary_fn: rt_unary,
    build_array_fn: rt_build_array,
    build_object_fn: rt_build_object,
    index_get_fn: rt_index_get,
    index_set_fn: rt_index_set,
    prop_get_fn: rt_prop_get,
    prop_set_fn: rt_prop_set,
    array_len_fn: rt_array_len,
    array_push_fn: rt_array_push,
    array_pop_fn: rt_array_pop,
    object_keys_fn: rt_object_keys,
    object_del_fn: rt_object_del,
    object_del_key_fn: rt_object_del_key,
    is_array_fn: rt_is_array,
    is_object_fn: rt_is_object,
    member_len_fn: rt_member_len,
    type_fn: rt_type,
    wait_fn: rt_wait,
    append_str_global_fn: rt_append_str_global,
    make_closure_fn: rt_make_closure,
    call_closure_fn: rt_call_closure,
    load_upvalue_fn: rt_load_upvalue,
    store_upvalue_fn: rt_store_upvalue,
    set_error_fn: rt_set_error,
    has_error_fn: rt_has_error,
};

extern "C" fn rt_print(ctx_ptr: *mut KriaContext, val: LlvmValue) {
    let ctx = unsafe { &mut *ctx_ptr };
    let value = ctx.to_value(val);
    println!("{}", value);
}

extern "C" fn rt_input(ctx_ptr: *mut KriaContext, prompt: LlvmValue, _type_mask: u8) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let prompt_val = ctx.to_value(prompt);
    println!("{}", prompt_val);
    let mut line = String::new();
    if let Err(e) = std::io::stdin().read_line(&mut line) {
        ctx.set_error(e.to_string());
        return LlvmValue::NULL;
    }
    if line.is_empty() {
        LlvmValue::NULL
    } else {
        let trimmed = line.trim();
        if let Ok(num) = trimmed.parse::<i64>() {
            LlvmValue::int(num)
        } else if let Ok(flt) = trimmed.parse::<f64>() {
            LlvmValue::float(flt)
        } else {
            ctx.from_value(Value::String(StringValue::from(trimmed)))
        }
    }
}

extern "C" fn rt_binop(
    ctx_ptr: *mut KriaContext,
    op_code: u32,
    left: LlvmValue,
    right: LlvmValue,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let left_val = ctx.to_value(left);
    let right_val = ctx.to_value(right);

    let op = match op_code {
        0 => BinaryOperator::Add,
        1 => BinaryOperator::Subtract,
        2 => BinaryOperator::Multiply,
        3 => BinaryOperator::Divide,
        4 => BinaryOperator::Equals,
        5 => BinaryOperator::NotEquals,
        6 => BinaryOperator::GreaterThan,
        7 => BinaryOperator::LessThan,
        8 => BinaryOperator::GreaterOrEqual,
        9 => BinaryOperator::LessOrEqual,
        10 => BinaryOperator::And,
        11 => BinaryOperator::Or,
        _ => {
            ctx.set_error(format!("Unknown binary operator code {}", op_code));
            return LlvmValue::NULL;
        }
    };

    match op {
        BinaryOperator::And => {
            let res = left.is_truthy() && right.is_truthy();
            LlvmValue::bool(res)
        }
        BinaryOperator::Or => {
            let res = left.is_truthy() || right.is_truthy();
            LlvmValue::bool(res)
        }
        BinaryOperator::Equals => {
            if left.tag == TAG_HEAP && right.tag == TAG_HEAP {
                let l_idx = left.payload as usize;
                let r_idx = right.payload as usize;
                if l_idx == r_idx {
                    return LlvmValue::bool(true);
                }
                if let (Some(Value::String(l)), Some(Value::String(r))) =
                    (ctx.heap_objects.get(l_idx), ctx.heap_objects.get(r_idx))
                {
                    return LlvmValue::bool(l.as_str() == r.as_str());
                }
            }
            let eq = left_val == right_val;
            LlvmValue::bool(eq)
        }
        BinaryOperator::NotEquals => {
            if left.tag == TAG_HEAP && right.tag == TAG_HEAP {
                let l_idx = left.payload as usize;
                let r_idx = right.payload as usize;
                if l_idx == r_idx {
                    return LlvmValue::bool(false);
                }
                if let (Some(Value::String(l)), Some(Value::String(r))) =
                    (ctx.heap_objects.get(l_idx), ctx.heap_objects.get(r_idx))
                {
                    return LlvmValue::bool(l.as_str() != r.as_str());
                }
            }
            let eq = left_val == right_val;
            LlvmValue::bool(!eq)
        }
        BinaryOperator::Add => match (&left_val, &right_val) {
            (Value::String(l), Value::String(r)) => {
                let s = StringValue::concat(l, r);
                ctx.from_value(Value::String(s))
            }
            _ => {
                let num_op = NumericOp::Add;
                match apply_numeric_op(num_op, &left_val, &right_val) {
                    Ok(v) => ctx.from_value(v),
                    Err(e) => {
                        ctx.set_error(format!("Add failed: {:?}", e));
                        LlvmValue::NULL
                    }
                }
            }
        },
        _ => {
            let num_op = match op {
                BinaryOperator::Subtract => NumericOp::Subtract,
                BinaryOperator::Multiply => NumericOp::Multiply,
                BinaryOperator::Divide => NumericOp::Divide,
                BinaryOperator::GreaterThan => NumericOp::Greater,
                BinaryOperator::LessThan => NumericOp::Less,
                BinaryOperator::GreaterOrEqual => NumericOp::GreaterEqual,
                BinaryOperator::LessOrEqual => NumericOp::LessEqual,
                _ => unreachable!(),
            };
            match apply_numeric_op(num_op, &left_val, &right_val) {
                Ok(v) => ctx.from_value(v),
                Err(e) => {
                    ctx.set_error(format!("Operation failed: {:?}", e));
                    LlvmValue::NULL
                }
            }
        }
    }
}

extern "C" fn rt_unary(ctx_ptr: *mut KriaContext, op_code: u32, val: LlvmValue) -> LlvmValue {
    let _ctx = unsafe { &mut *ctx_ptr };
    match op_code {
        0 => {
            // Not
            LlvmValue::bool(!val.is_truthy())
        }
        _ => LlvmValue::NULL,
    }
}

extern "C" fn rt_build_array(
    ctx_ptr: *mut KriaContext,
    elements_ptr: *const LlvmValue,
    count: u32,
    mutable: u32,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let mut values = Vec::with_capacity(count as usize);
    for i in 0..count {
        let item = unsafe { *elements_ptr.add(i as usize) };
        values.push(ctx.to_value(item));
    }
    let is_mut = mutable != 0;
    let data = ArrayData {
        mutable: is_mut,
        elements: if is_mut {
            ArrayElements::Mutable(RefCell::new(values))
        } else {
            ArrayElements::Immutable(values)
        },
    };
    ctx.from_value(Value::Array(Rc::new(data)))
}

extern "C" fn rt_build_object(
    ctx_ptr: *mut KriaContext,
    keys_ptr: *const *const u8,
    lens_ptr: *const u32,
    values_ptr: *const LlvmValue,
    count: u32,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let object = new_object();
    if let Value::Object(obj_data) = &object {
        let mut fields = obj_data.fields.borrow_mut();
        for i in 0..count {
            let k_ptr = unsafe { *keys_ptr.add(i as usize) };
            let k_len = unsafe { *lens_ptr.add(i as usize) } as usize;
            let slice = unsafe { std::slice::from_raw_parts(k_ptr, k_len) };
            let key_str = std::str::from_utf8(slice).unwrap_or("");
            let val = unsafe { *values_ptr.add(i as usize) };
            fields.insert(Arc::from(key_str), ctx.to_value(val));
        }
    }
    ctx.from_value(object)
}

extern "C" fn rt_index_get(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    index: LlvmValue,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let res = if object.tag != TAG_HEAP {
        Value::Null
    } else {
        match (ctx.heap_objects.get(object.payload as usize), index.tag) {
            (Some(Value::Array(array)), TAG_INT) => {
                let idx = index.payload as i64;
                if idx >= 0 {
                    array.get_cloned(idx as usize).unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
            (Some(Value::Object(obj)), TAG_HEAP) => {
                match ctx.heap_objects.get(index.payload as usize) {
                    Some(Value::String(key)) => obj
                        .fields
                        .borrow()
                        .get(key.as_str())
                        .cloned()
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                }
            }
            _ => Value::Null,
        }
    };
    ctx.from_value(res)
}

extern "C" fn rt_index_set(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    index: LlvmValue,
    value_ptr: *const LlvmValue,
) {
    let ctx = unsafe { &mut *ctx_ptr };
    let val = ctx.to_value(unsafe { *value_ptr });

    if object.tag != TAG_HEAP {
        return;
    }
    match (ctx.heap_objects.get(object.payload as usize), index.tag) {
        (Some(Value::Array(arr)), TAG_INT) => {
            let idx = index.payload as i64;
            if idx >= 0 {
                let _ = arr.set_at(idx as usize, val);
            }
        }
        (Some(Value::Object(o)), TAG_HEAP) => {
            let key = match ctx.heap_objects.get(index.payload as usize) {
                Some(Value::String(key)) => key.to_arc_str(),
                _ => return,
            };
            let mut fields = o.fields.borrow_mut();
            let is_new = !fields.contains_key(key.as_ref());
            fields.insert(key, val);
            if is_new {
                o.shape_version.set(o.shape_version.get().wrapping_add(1));
            }
        }
        _ => {}
    }
}

extern "C" fn rt_prop_get(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    site: u32,
    member_ptr: *const u8,
    member_len: u32,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let slice = unsafe { std::slice::from_raw_parts(member_ptr, member_len as usize) };
    let member = std::str::from_utf8(slice).unwrap_or("");
    let object_index = object.payload as usize;
    let res = if object.tag != TAG_HEAP {
        Value::Null
    } else if let Some(Value::Object(obj)) = ctx.heap_objects.get(object_index) {
        let version = obj.shape_version.get();
        let cached = ctx
            .property_cache
            .get(&(site, object_index))
            .filter(|(cached_version, _)| *cached_version == version)
            .map(|(_, index)| *index);
        let fields = obj.fields.borrow();
        if let Some(value) = cached.and_then(|index| fields.get_index(index).map(|(_, v)| v)) {
            value.clone()
        } else if let Some(index) = fields.get_index_of(member) {
            let value = fields.get_index(index).map(|(_, value)| value.clone());
            if let Some(value) = value {
                ctx.property_cache
                    .insert((site, object_index), (version, index));
                value
            } else {
                Value::Null
            }
        } else {
            Value::Null
        }
    } else {
        Value::Null
    };
    ctx.from_value(res)
}

extern "C" fn rt_prop_set(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    member_ptr: *const u8,
    member_len: u32,
    value_ptr: *const LlvmValue,
) {
    let ctx = unsafe { &mut *ctx_ptr };
    let slice = unsafe { std::slice::from_raw_parts(member_ptr, member_len as usize) };
    let member = std::str::from_utf8(slice).unwrap_or("");
    let obj = ctx.to_value(object);
    let val = ctx.to_value(unsafe { *value_ptr });

    if let Value::Object(o) = &obj {
        let mut fields = o.fields.borrow_mut();
        let is_new = !fields.contains_key(member);
        fields.insert(Arc::from(member), val);
        if is_new {
            o.shape_version.set(o.shape_version.get().wrapping_add(1));
        }
    }
}

extern "C" fn rt_array_len(ctx_ptr: *mut KriaContext, object: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let len = if object.tag == TAG_HEAP {
        match ctx.heap_objects.get(object.payload as usize) {
            Some(Value::Array(array)) => array.len() as i64,
            _ => 0,
        }
    } else {
        0
    };
    LlvmValue::int(len)
}

extern "C" fn rt_array_push(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    value_ptr: *const LlvmValue,
) {
    let ctx = unsafe { &mut *ctx_ptr };
    let obj = ctx.to_value(object);
    let val = ctx.to_value(unsafe { *value_ptr });
    if let Value::Array(a) = &obj {
        let _ = a.push(val);
    }
}

extern "C" fn rt_array_pop(ctx_ptr: *mut KriaContext, object: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let obj = ctx.to_value(object);
    let res = match &obj {
        Value::Array(a) => a.pop().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    ctx.from_value(res)
}

extern "C" fn rt_object_keys(ctx_ptr: *mut KriaContext, object: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let obj = ctx.to_value(object);
    let keys = match &obj {
        Value::Object(o) => o
            .fields
            .borrow()
            .keys()
            .map(|k| Value::String(StringValue::from(k.as_ref())))
            .collect(),
        _ => Vec::new(),
    };
    let data = ArrayData {
        mutable: true,
        elements: ArrayElements::Mutable(RefCell::new(keys)),
    };
    ctx.from_value(Value::Array(Rc::new(data)))
}

extern "C" fn rt_object_del(
    ctx_ptr: *mut KriaContext,
    object: LlvmValue,
    member_ptr: *const u8,
    member_len: u32,
) {
    let ctx = unsafe { &mut *ctx_ptr };
    let slice = unsafe { std::slice::from_raw_parts(member_ptr, member_len as usize) };
    let member = std::str::from_utf8(slice).unwrap_or("");
    let obj = ctx.to_value(object);
    if let Value::Object(o) = &obj {
        if o.fields.borrow_mut().shift_remove(member).is_some() {
            o.shape_version.set(o.shape_version.get().wrapping_add(1));
        }
    }
}

extern "C" fn rt_object_del_key(ctx_ptr: *mut KriaContext, object: LlvmValue, key: LlvmValue) {
    let ctx = unsafe { &mut *ctx_ptr };
    let obj = ctx.to_value(object);
    let k = ctx.to_value(key);
    if let (Value::Object(o), Value::String(s)) = (&obj, &k) {
        if o.fields
            .borrow_mut()
            .shift_remove(s.to_arc_str().as_ref())
            .is_some()
        {
            o.shape_version.set(o.shape_version.get().wrapping_add(1));
        }
    }
}

extern "C" fn rt_is_array(ctx_ptr: *mut KriaContext, value: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let v = ctx.to_value(value);
    LlvmValue::bool(matches!(v, Value::Array(_)))
}

extern "C" fn rt_is_object(ctx_ptr: *mut KriaContext, value: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let v = ctx.to_value(value);
    LlvmValue::bool(matches!(v, Value::Object(_)))
}

extern "C" fn rt_member_len(ctx_ptr: *mut KriaContext, object: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let obj = ctx.to_value(object);
    let len = match &obj {
        Value::Array(a) => a.len() as i64,
        Value::String(s) => s.len() as i64,
        Value::Object(o) => o.fields.borrow().len() as i64,
        _ => 0,
    };
    LlvmValue::int(len)
}

extern "C" fn rt_type(ctx_ptr: *mut KriaContext, value: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let v = ctx.to_value(value);
    let name = match v {
        Value::Number(_) | Value::Float(_) => "number",
        Value::String(_) => "string",
        Value::Boolean(_) => "boolean",
        Value::Null => "null",
        #[cfg(test)]
        Value::Function { .. } => "function",
        Value::RegisterFunction { .. } => "function",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    ctx.from_value(Value::String(StringValue::from(name)))
}

extern "C" fn rt_wait(ctx_ptr: *mut KriaContext, value: LlvmValue) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let v = ctx.to_value(value);
    if let Value::Number(ms) = v {
        std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
    }
    LlvmValue::NULL
}

extern "C" fn rt_append_str_global(
    ctx_ptr: *mut KriaContext,
    slot: u32,
    suffix_ptr: *const u8,
    suffix_len: u32,
) {
    let ctx = unsafe { &mut *ctx_ptr };
    let slice = unsafe { std::slice::from_raw_parts(suffix_ptr, suffix_len as usize) };
    let suffix = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return,
    };
    let slot = slot as usize;
    if slot < ctx.globals.len() {
        let val = ctx.globals[slot];
        if val.tag == TAG_HEAP {
            let idx = val.payload as usize;
            if idx < ctx.num_constants {
                // Pointing to constant pool; copy-on-write so constant pool is not mutated
                if let Some(Value::String(s)) = ctx.heap_objects.get(idx) {
                    let mut new_s = s.clone();
                    new_s.append(suffix);
                    let new_idx = ctx.heap_objects.len();
                    ctx.heap_objects.push(Value::String(new_s));
                    ctx.globals[slot] = LlvmValue::heap(new_idx);
                    return;
                }
            } else {
                // Dedicated heap object owned by the variable; mutate in-place
                if let Some(Value::String(s)) = ctx.heap_objects.get_mut(idx) {
                    s.append(suffix);
                    return;
                }
            }
        }
        ctx.set_error("Append requires string global".to_string());
    }
}

extern "C" fn rt_make_closure(
    ctx_ptr: *mut KriaContext,
    func_idx: u32,
    captures_ptr: *const LlvmValue,
    capture_kinds: *const u8,
    count: u32,
    cur_captures_id: u64,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let mut resolved = Vec::with_capacity(count as usize);

    for i in 0..count {
        let kind = unsafe { *capture_kinds.add(i as usize) };
        let val = if kind == 0 {
            let llvm_val = unsafe { *captures_ptr.add(i as usize) };
            ctx.to_value(llvm_val)
        } else {
            let up_slot = unsafe { (*captures_ptr.add(i as usize)).payload as usize };
            if cur_captures_id != 0 {
                let cell = unsafe { &*(cur_captures_id as *const RefCell<Vec<Value>>) };
                cell.borrow().get(up_slot).cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        };
        resolved.push(val);
    }

    let cell = Rc::new(RefCell::new(resolved));
    let reg_fn = Value::RegisterFunction {
        function_index: func_idx as usize,
        captures: cell,
    };
    ctx.from_value(reg_fn)
}

extern "C" fn rt_call_closure(
    ctx_ptr: *mut KriaContext,
    rt_ptr: *const KriaRuntime,
    callee: LlvmValue,
    args_ptr: *const LlvmValue,
    num_args: u32,
) -> LlvmValue {
    let ctx = unsafe { &mut *ctx_ptr };
    let closure = if callee.tag == TAG_HEAP {
        match ctx.heap_objects.get(callee.payload as usize) {
            Some(Value::RegisterFunction {
                function_index,
                captures,
            }) => Some((*function_index, Rc::as_ptr(captures) as u64)),
            _ => None,
        }
    } else {
        None
    };
    if let Some((function_index, captures_ptr)) = closure {
        if function_index < ctx.function_ptrs.len() {
            let func = ctx.function_ptrs[function_index];
            func(ctx_ptr, rt_ptr, captures_ptr, args_ptr, num_args)
        } else {
            ctx.set_error(format!(
                "Invalid register function index {}",
                function_index
            ));
            LlvmValue::NULL
        }
    } else {
        ctx.set_error("Called non-function".to_string());
        LlvmValue::NULL
    }
}

extern "C" fn rt_load_upvalue(
    _ctx_ptr: *mut KriaContext,
    captures_ptr: u64,
    slot: u32,
) -> LlvmValue {
    if captures_ptr != 0 {
        let cell = unsafe { &*(captures_ptr as *const RefCell<Vec<Value>>) };
        let borrowed = cell.borrow();
        if let Some(val) = borrowed.get(slot as usize) {
            let ctx = unsafe { &mut *_ctx_ptr };
            return ctx.from_value(val.clone());
        }
    }
    LlvmValue::NULL
}

extern "C" fn rt_store_upvalue(
    ctx_ptr: *mut KriaContext,
    captures_ptr: u64,
    slot: u32,
    val_ptr: *const LlvmValue,
) {
    if captures_ptr != 0 {
        let ctx = unsafe { &mut *ctx_ptr };
        let value = ctx.to_value(unsafe { *val_ptr });
        let cell = unsafe { &*(captures_ptr as *const RefCell<Vec<Value>>) };
        let mut borrowed = cell.borrow_mut();
        let slot = slot as usize;
        if slot >= borrowed.len() {
            borrowed.resize(slot + 1, Value::Null);
        }
        borrowed[slot] = value;
    }
}

extern "C" fn rt_set_error(ctx_ptr: *mut KriaContext, err_ptr: *const u8, err_len: u32) {
    let ctx = unsafe { &mut *ctx_ptr };
    let slice = unsafe { std::slice::from_raw_parts(err_ptr, err_len as usize) };
    let msg = std::str::from_utf8(slice)
        .unwrap_or("Runtime error")
        .to_string();
    ctx.set_error(msg);
}

extern "C" fn rt_has_error(ctx_ptr: *mut KriaContext) -> u32 {
    let ctx = unsafe { &*ctx_ptr };
    if ctx.error.is_some() {
        1
    } else {
        0
    }
}
