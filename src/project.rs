use std::collections::HashMap;
use std::path::PathBuf;

use crate::bytecode::Bytecode;
use crate::compiler::Compiler;
use crate::modules::{collect_imports, load_module_graph, resolve_import_path, ModuleGraph};
use crate::optimizer::optimize;

pub fn compile_entry_file(entry: &std::path::Path) -> Result<Bytecode, String> {
    let graph = load_module_graph(entry)?;
    compile_graph(&graph)
}

pub fn compile_graph(graph: &ModuleGraph) -> Result<Bytecode, String> {
    let mut compiler = Compiler::new();
    let mut exports_by_path: HashMap<PathBuf, HashMap<String, usize>> = HashMap::new();

    for module in &graph.modules {
        compiler.begin_module();

        for (alias, import_path) in collect_imports(&module.statements) {
            let dep_path = resolve_import_path(&module.path, &import_path)?;
            let exports = exports_by_path.get(&dep_path).ok_or_else(|| {
                format!(
                    "Internal error: module '{}' not compiled before '{}'",
                    dep_path.display(),
                    module.path.display()
                )
            })?;
            compiler.bind_import(&alias, exports)?;
        }

        let module_exports = compiler.compile_module(&module.statements)?;
        exports_by_path.insert(module.path.clone(), module_exports);
    }

    Ok(optimize(compiler.finish_bytecode()))
}
