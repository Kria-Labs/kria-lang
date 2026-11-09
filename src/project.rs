use crate::modules::{collect_imports, load_module_graph, resolve_import_path};

pub fn compile_register_entry_file(
    entry: &std::path::Path,
) -> Result<crate::register::SsaModule, String> {
    let graph = load_module_graph(entry)?;
    let mut lowerer = crate::register::ModuleLowerer::new();
    let mut exports_by_path: std::collections::HashMap<
        std::path::PathBuf,
        std::collections::HashMap<String, u32>,
    > = std::collections::HashMap::new();

    let mut all_statements = Vec::new();
    for module in &graph.modules {
        for (alias, import_path) in collect_imports(&module.statements) {
            let dep_path = resolve_import_path(&module.path, &import_path)?;
            let exports = exports_by_path.get(&dep_path).ok_or_else(|| {
                format!(
                    "Internal error: module '{}' not compiled before '{}'",
                    dep_path.display(),
                    module.path.display()
                )
            })?;
            lowerer.bind_import(&alias, exports.clone());
        }

        let mut module_exports = std::collections::HashMap::new();
        for statement in &module.statements {
            if let crate::ast::Statement::FunctionDef {
                name,
                exported: true,
                ..
            } = statement
            {
                let slot = lowerer.resolve_global(name);
                module_exports.insert(name.clone(), slot);
            }
        }
        exports_by_path.insert(module.path.clone(), module_exports);
        all_statements.extend(module.statements.clone());
    }

    lowerer.lower_module(&all_statements)
}
