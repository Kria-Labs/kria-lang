use std::collections::HashMap;
use std::path::PathBuf;

use crate::modules::{collect_imports, load_module_graph, resolve_import_path};
use crate::register::ModuleUnit;

pub fn compile_register_entry_file(
    entry: &std::path::Path,
) -> Result<crate::register::SsaModule, String> {
    let graph = load_module_graph(entry)?;
    let index_by_path: HashMap<PathBuf, usize> = graph
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.path.clone(), index))
        .collect();

    let mut units = Vec::with_capacity(graph.modules.len());
    for (index, module) in graph.modules.iter().enumerate() {
        let mut imports = Vec::new();
        for (alias, import_path) in collect_imports(&module.statements) {
            let dep_path = resolve_import_path(&module.path, &import_path)?;
            let dep = *index_by_path.get(&dep_path).ok_or_else(|| {
                format!(
                    "Internal error: module '{}' was not loaded",
                    dep_path.display()
                )
            })?;
            if dep >= index {
                return Err(format!(
                    "Internal error: module '{}' not compiled before '{}'",
                    dep_path.display(),
                    module.path.display()
                ));
            }
            imports.push((alias, dep));
        }
        let name = module
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("module")
            .to_string();
        units.push(ModuleUnit {
            name,
            imports,
            statements: module.statements.clone(),
        });
    }

    if graph.entry_index + 1 != units.len() {
        return Err("Internal error: entry module is not last in the module graph".to_string());
    }

    let mut lowerer = crate::register::ModuleLowerer::new();
    lowerer.lower_program(&units)
}

#[cfg(test)]
mod tests {
    use super::compile_register_entry_file;
    use crate::vm::{RegisterVM, Value};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn write_project(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kria_module_test_{}_{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        dir
    }

    fn run_globals(dir: &Path, names: &[&str]) -> Vec<Value> {
        let module = compile_register_entry_file(&dir.join("main.krx")).unwrap();
        let mut vm = RegisterVM::new();
        vm.execute(&module).unwrap();
        let values = names
            .iter()
            .map(|name| match module.globals.get(*name) {
                Some(slot) => vm.global(*slot as usize),
                None => Value::Null,
            })
            .collect();
        let _ = std::fs::remove_dir_all(dir);
        values
    }

    fn assert_str(value: &Value, expected: &str) {
        match value {
            Value::String(s) => assert_eq!(s.to_string(), expected),
            other => panic!("expected string {:?}, got {:?}", expected, other),
        }
    }

    const LIB: &str = "set counter = 100\n\
        fn helper() { return \"lib helper\" }\n\
        export fn api() { return helper() }\n\
        export fn get_counter() { return counter }\n\
        export fn bump() {\nset counter = counter + 1\nreturn counter\n}\n";

    #[test]
    fn private_function_with_same_name_does_not_clash() {
        let dir = write_project(&[
            ("lib.krx", LIB),
            (
                "main.krx",
                "import lib from \"./lib.krx\"\n\
                 fn helper() { return \"main helper\" }\n\
                 set a = lib.api()\n\
                 set b = helper()\n",
            ),
        ]);
        let values = run_globals(&dir, &["a", "b"]);
        assert_str(&values[0], "lib helper");
        assert_str(&values[1], "main helper");
    }

    #[test]
    fn module_top_level_variables_do_not_leak() {
        let dir = write_project(&[
            ("lib.krx", LIB),
            (
                "main.krx",
                "import lib from \"./lib.krx\"\nset leaked = counter\n",
            ),
        ]);
        let values = run_globals(&dir, &["leaked"]);
        assert!(matches!(values[0], Value::Null));
    }

    #[test]
    fn module_keeps_its_own_state_separate_from_importer() {
        let dir = write_project(&[
            ("lib.krx", LIB),
            (
                "main.krx",
                "import lib from \"./lib.krx\"\n\
                 set counter = 1\n\
                 set first = lib.bump()\n\
                 set second = lib.bump()\n\
                 set seen = lib.get_counter()\n",
            ),
        ]);
        let values = run_globals(&dir, &["counter", "first", "second", "seen"]);
        assert!(matches!(values[0], Value::Number(1)));
        assert!(matches!(values[1], Value::Number(101)));
        assert!(matches!(values[2], Value::Number(102)));
        assert!(matches!(values[3], Value::Number(102)));
    }

    #[test]
    fn exported_and_private_names_are_not_global_in_importer() {
        let dir = write_project(&[
            ("lib.krx", LIB),
            (
                "main.krx",
                "import lib from \"./lib.krx\"\n\
                 set unqualified = api\n\
                 set private_member = lib.helper\n\
                 set private_global = helper\n",
            ),
        ]);
        let values = run_globals(&dir, &["unqualified", "private_member", "private_global"]);
        assert!(values.iter().all(|value| matches!(value, Value::Null)));
    }

    #[test]
    fn nested_imports_have_separate_namespaces() {
        let dir = write_project(&[
            (
                "sub/util.krx",
                "fn helper() { return \"util\" }\nexport fn name() { return helper() }\n",
            ),
            (
                "other.krx",
                "import u from \"./sub/util.krx\"\n\
                 fn helper() { return \"other\" }\n\
                 export fn describe() { return helper() + \"/\" + u.name() }\n",
            ),
            (
                "main.krx",
                "import other from \"./other.krx\"\n\
                 fn helper() { return \"main\" }\n\
                 set r = other.describe() + \"/\" + helper()\n",
            ),
        ]);
        let values = run_globals(&dir, &["r"]);
        assert_str(&values[0], "other/util/main");
    }

    #[test]
    fn same_alias_in_two_modules_refers_to_each_modules_import() {
        let dir = write_project(&[
            ("a.krx", "export fn who() { return \"a\" }\n"),
            ("b.krx", "export fn who() { return \"b\" }\n"),
            (
                "mid.krx",
                "import m from \"./b.krx\"\nexport fn via() { return m.who() }\n",
            ),
            (
                "main.krx",
                "import m from \"./a.krx\"\n\
                 import mid from \"./mid.krx\"\n\
                 set r = m.who() + mid.via()\n",
            ),
        ]);
        let values = run_globals(&dir, &["r"]);
        assert_str(&values[0], "ab");
    }

    #[test]
    fn shared_dependency_is_loaded_once() {
        let dir = write_project(&[
            (
                "counter.krx",
                "set n = 0\nexport fn next() {\nset n = n + 1\nreturn n\n}\n",
            ),
            (
                "left.krx",
                "import c from \"./counter.krx\"\nexport fn go() { return c.next() }\n",
            ),
            (
                "main.krx",
                "import c from \"./counter.krx\"\n\
                 import left from \"./left.krx\"\n\
                 set x = left.go()\n\
                 set y = c.next()\n",
            ),
        ]);
        let values = run_globals(&dir, &["x", "y"]);
        assert!(matches!(values[0], Value::Number(1)));
        assert!(matches!(values[1], Value::Number(2)));
    }
}
