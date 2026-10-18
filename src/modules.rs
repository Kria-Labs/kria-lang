use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::Statement;
use crate::lexer::Lexer;
use crate::parser::Parser;

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub path: PathBuf,
    pub statements: Vec<Statement>,
}

#[derive(Debug)]
pub struct ModuleGraph {
    pub modules: Vec<ResolvedModule>,
    pub entry_index: usize,
}

pub fn load_module_graph(entry: &Path) -> Result<ModuleGraph, String> {
    let entry = entry
        .canonicalize()
        .map_err(|e| format!("Cannot resolve entry file '{}': {}", entry.display(), e))?;

    if entry.extension().and_then(|e| e.to_str()) != Some("krx") {
        return Err(format!(
            "Entry file must be a .krx file: {}",
            entry.display()
        ));
    }

    let mut loaded: HashMap<PathBuf, Vec<Statement>> = HashMap::new();
    let mut order: Vec<PathBuf> = Vec::new();
    let mut visiting: HashSet<PathBuf> = HashSet::new();

    fn visit(
        path: &Path,
        loaded: &mut HashMap<PathBuf, Vec<Statement>>,
        order: &mut Vec<PathBuf>,
        visiting: &mut HashSet<PathBuf>,
    ) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path '{}': {}", path.display(), e))?;

        if loaded.contains_key(&path) {
            return Ok(());
        }

        if visiting.contains(&path) {
            return Err(format!(
                "Circular import detected involving {}",
                path.display()
            ));
        }

        visiting.insert(path.clone());

        let source = std::fs::read_to_string(&path)
            .map_err(|e| format!("Error reading '{}': {}", path.display(), e))?;

        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse()?;

        for stmt in &statements {
            if let Statement::Import {
                path: import_path, ..
            } = stmt
            {
                let resolved = resolve_import_path(&path, import_path)?;
                visit(&resolved, loaded, order, visiting)?;
            }
        }

        visiting.remove(&path);
        loaded.insert(path.clone(), statements);
        order.push(path);
        Ok(())
    }

    visit(&entry, &mut loaded, &mut order, &mut visiting)?;

    let entry_canon = entry.clone();
    let entry_index = order
        .iter()
        .position(|p| p == &entry_canon)
        .ok_or_else(|| "Entry module not found in graph".to_string())?;

    let modules = order
        .into_iter()
        .map(|path| {
            let statements = loaded.remove(&path).unwrap();
            ResolvedModule { path, statements }
        })
        .collect();

    Ok(ModuleGraph {
        modules,
        entry_index,
    })
}

pub fn resolve_import_path(importer: &Path, import_path: &str) -> Result<PathBuf, String> {
    if !import_path.ends_with(".krx") {
        return Err(format!(
            "Import path must be a .krx file: '{}'",
            import_path
        ));
    }

    let parent = importer.parent().unwrap_or_else(|| Path::new("."));
    let joined = parent.join(import_path);
    let canonical = joined
        .canonicalize()
        .map_err(|_| format!("Cannot find module file '{}'", joined.display()))?;

    Ok(canonical)
}

pub fn collect_imports(statements: &[Statement]) -> Vec<(String, String)> {
    statements
        .iter()
        .filter_map(|s| match s {
            Statement::Import { alias, path } => Some((alias.clone(), path.clone())),
            _ => None,
        })
        .collect()
}
