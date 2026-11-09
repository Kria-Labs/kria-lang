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
