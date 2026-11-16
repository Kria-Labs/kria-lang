use super::*;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::register::ModuleLowerer;

fn compile_to_ssa(source: &str) -> crate::register::SsaModule {
    let tokens = Lexer::new(source).tokenize();
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse().unwrap();
    let mut lowerer = ModuleLowerer::new();
    lowerer.lower_module(&stmts).unwrap()
}

#[test]
fn test_jit_arithmetic_and_print() {
    let ssa = compile_to_ssa("set x = 10\nset y = 20\nset z = x + y\nprint(z)\n");
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_float_fast_paths() {
    let ssa = compile_to_ssa(
        "set left = 1.5\nset right = 2.25\nprint(left + right)\nprint(left < right)\nprint(left / right)\n",
    );
    let jit = LlvmJit::new();
    let ir = jit.compile_to_llvm_ir(&ssa);
    assert!(ir.contains("fadd double"));
    assert!(ir.contains("fcmp olt double"));
    jit.compile(&ssa).unwrap().run().unwrap();
}

#[test]
fn test_jit_compiled_artifact_cache_hit() {
    let ssa = compile_to_ssa("set value = 42\nprint(value)\n");
    let jit = LlvmJit::new();
    jit.compile(&ssa).unwrap().run().unwrap();
    jit.compile(&ssa).unwrap().run().unwrap();
}

#[test]
fn test_jit_loads_and_runs_platform_library() {
    let ssa = compile_to_ssa("set value = 42\n");
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_function_constants_use_the_shared_constant_pool_offset() {
    let ssa = compile_to_ssa(
        r#"
print("entry string")
fn greet() {
    print("function string")
}
greet()
"#,
    );
    let ir = LlvmJit::new().compile_to_llvm_ir(&ssa);
    let function_ir = ir
        .split("define %Value @kria_func_direct_0")
        .nth(1)
        .unwrap()
        .split("define %Value @kria_func_0")
        .next()
        .unwrap();
    assert!(function_ir.lines().any(|line| {
        line.contains("getelementptr %Value, %Value* %c_ptr_") && line.ends_with("i32 1")
    }));
    assert_eq!(ssa.constants.len(), 1);
    assert_eq!(ssa.functions[0].constants.len(), 1);
}

#[test]
fn test_jit_repeated_for_in_bindings_allocate_every_local_slot() {
    let ssa = compile_to_ssa(
        r#"
set values = [1, 2]
for item in values {
    print(item)
}
for item in values {
    print(item)
}
"#,
    );
    let mut jit = LlvmJit::new();
    jit.compile(&ssa).unwrap();
}

#[test]
fn test_jit_while_loop_accumulator() {
    let ssa = compile_to_ssa(
        r#"
set sum = 0
set i = 0
while (i < 100) {
    set sum = sum + i
    set i = i + 1
}
print(sum)
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_conditionals() {
    let ssa = compile_to_ssa(
        r#"
set a = 5
set b = 10
if (a > b) {
    print("greater")
} elseif (a == b) {
    print("equal")
} else {
    print("less")
}
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_function_calls() {
    let ssa = compile_to_ssa(
        r#"
fn add(a, b) {
    return a + b
}
set res = add(15, 27)
print(res)
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_arrays() {
    let ssa = compile_to_ssa(
        r#"
set arr = [1, 2, 3]
push(arr, 4)
print(arr.length)
print(arr[0])
print(arr[3])
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_objects() {
    let ssa = compile_to_ssa(
        r#"
set obj = { name: "kria", ver: 1 }
print(obj.name)
print(obj.ver)
obj.ver = 2
print(obj.ver)
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_closure() {
    let ssa = compile_to_ssa(
        r#"
set factor = 5
set mult = fn(x) {
    return x * factor
}
print(mult(6))
"#,
    );
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

#[test]
fn test_jit_repeated_closure_calls() {
    let ssa = compile_to_ssa(
        r#"
set factor = 3
set multiply = fn(x) {
    return x * factor
}
set sum = 0
set i = 0
while (i < 100000) {
    set sum = sum + multiply(i)
    set i = i + 1
}
print(sum)
"#,
    );
    LlvmJit::new().execute(&ssa).unwrap();
}

#[test]
fn test_jit_runs_test_krx() {
    let path = std::path::Path::new("test.krx");
    let ssa = crate::project::compile_register_entry_file(path).unwrap();
    let mut jit = LlvmJit::new();
    jit.execute(&ssa).unwrap();
}

fn run_jit_globals(source: &str) -> (Result<(), String>, impl Fn(&str) -> crate::vm::Value) {
    let ssa = compile_to_ssa(source);
    let compiled = LlvmJit::new().compile(&ssa).unwrap();
    let (result, globals) = compiled.run_collect_globals();
    let slots = ssa.globals.clone();
    let lookup = move |name: &str| {
        slots
            .get(name)
            .and_then(|slot| globals.get(*slot as usize).cloned())
            .unwrap_or(crate::vm::Value::Null)
    };
    (result, lookup)
}

#[test]
fn test_jit_division_by_zero_stops_execution() {
    let (result, global) =
        run_jit_globals("set before = 1\nset z = 0\nset q = 10 / z\nset after = 1\n");
    assert_eq!(result, Err("Division by zero".to_string()));
    assert!(matches!(global("before"), crate::vm::Value::Number(1)));
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_literal_division_by_zero_is_reported() {
    let (result, global) = run_jit_globals("set q = 1 / 0\nset after = 1\n");
    assert_eq!(result, Err("Division by zero".to_string()));
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_min_divided_by_minus_one_wraps_like_the_vm() {
    let (result, global) =
        run_jit_globals("set m = -9223372036854775807 - 1\nset n = -1\nset q = m / n\n");
    assert_eq!(result, Ok(()));
    assert!(matches!(global("q"), crate::vm::Value::Number(i64::MIN)));
}

#[test]
fn test_jit_error_in_callee_stops_caller() {
    let (result, global) = run_jit_globals(
        "set progress = 0\n\
         fn inner(x) {\nset q = 10 / x\nreturn q\n}\n\
         fn outer(x) {\nset r = inner(x)\nset progress = 99\nreturn r\n}\n\
         set r = outer(0)\nset after = 1\n",
    );
    assert_eq!(result, Err("Division by zero".to_string()));
    assert!(matches!(global("progress"), crate::vm::Value::Number(0)));
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_type_error_stops_loop() {
    let (result, global) = run_jit_globals(
        "set i = 0\nwhile (i < 100) {\nset i = i + 1\nif (i == 3) {\nset bad = i + null\n}\n}\nset after = 1\n",
    );
    assert_eq!(
        result,
        Err("Add requires two numbers or two strings, found Number(3) and Null".to_string())
    );
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_calling_non_function_stops_execution() {
    let (result, global) = run_jit_globals("set x = 5\nx(1)\nset after = 1\n");
    assert_eq!(
        result,
        Err("Attempted to call non-function value".to_string())
    );
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_recursion_limit_matches_the_vm() {
    let depth = crate::vm::MAX_CALL_DEPTH - 1;
    let (result, global) = run_jit_globals(&format!(
        "fn f(n) {{\nif (n == 0) {{ return 0 }}\nreturn 1 + f(n - 1)\n}}\nset r = f({})\n",
        depth
    ));
    assert_eq!(result, Ok(()));
    assert!(matches!(global("r"), crate::vm::Value::Number(n) if n == depth as i64));

    let (result, global) = run_jit_globals(
        "fn f(n) {\nif (n == 0) { return 0 }\nreturn 1 + f(n - 1)\n}\nset r = f(100000)\nset after = 1\n",
    );
    assert_eq!(result, Err(crate::vm::recursion_limit_message()));
    assert!(matches!(global("after"), crate::vm::Value::Null));
}

#[test]
fn test_jit_call_depth_is_released_after_returning() {
    let (result, global) = run_jit_globals(
        "fn f(n) {\nif (n == 0) { return 0 }\nreturn 1 + f(n - 1)\n}\n\
         set i = 0\nset total = 0\nwhile (i < 20) {\nset total = total + f(900)\nset i = i + 1\n}\n",
    );
    assert_eq!(result, Ok(()));
    assert!(matches!(global("total"), crate::vm::Value::Number(18000)));
}
