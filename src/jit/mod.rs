pub mod compiler;
pub mod runtime;
pub mod value;

#[cfg(test)]
mod tests;

pub use compiler::LlvmCompiler;
pub use runtime::{KriaContext, KriaRuntime, RUNTIME_TABLE};
pub use value::LlvmValue;

use crate::register::SsaModule;
use libloading::Library;
use std::fs;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

static JIT_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);
static CLANG_FINGERPRINT: OnceLock<Result<String, String>> = OnceLock::new();

pub struct LlvmJit {
    opt_level: u32,
}

pub struct CompiledJit {
    library: Option<Library>,
    _artifacts: JitArtifacts,
    entry_fn: extern "C" fn(*mut KriaContext, *const KriaRuntime),
    function_ptrs: Vec<runtime::JitFn>,
    globals_count: usize,
    constants: Vec<crate::vm::Value>,
}

struct JitArtifacts {
    ir_path: Option<std::path::PathBuf>,
    library_path: Option<std::path::PathBuf>,
    import_library_path: Option<std::path::PathBuf>,
}

impl Drop for JitArtifacts {
    fn drop(&mut self) {
        if let Some(path) = &self.ir_path {
            let _ = fs::remove_file(path);
        }
        if let Some(path) = &self.library_path {
            let _ = fs::remove_file(path);
        }
        if let Some(path) = &self.import_library_path {
            let _ = fs::remove_file(path);
        }
    }
}

impl CompiledJit {
    pub fn run(&self) -> Result<(), String> {
        let mut ctx = KriaContext::new(self.globals_count, &self.constants);
        ctx.set_function_ptrs(self.function_ptrs.clone());
        (self.entry_fn)(&mut ctx, &RUNTIME_TABLE);

        if let Some(err) = ctx.error {
            return Err(err);
        }
        Ok(())
    }
}

impl Drop for CompiledJit {
    fn drop(&mut self) {
        drop(self.library.take());
    }
}

impl LlvmJit {
    pub fn new() -> Self {
        Self { opt_level: 3 }
    }

    pub fn with_opt_level(opt_level: u32) -> Self {
        Self { opt_level }
    }

    pub fn compile_to_llvm_ir(&self, module: &SsaModule) -> String {
        LlvmCompiler::new(module).compile()
    }

    pub fn compile(&self, module: &SsaModule) -> Result<CompiledJit, String> {
        let ir = self.compile_to_llvm_ir(module);

        if std::env::var("KRIA_DUMP_LLVM").ok().as_deref() == Some("1") {
            eprintln!("=== GENERATED LLVM IR ===");
            eprintln!("{}", ir);
            eprintln!("=========================");
        }

        let pid = std::process::id();
        let count = JIT_FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let base_name = format!("kria_jit_{}_{}", pid, count);
        let cache_dir = temp_dir.join("kria-jit-cache-v1");
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create JIT cache directory: {}", e))?;
        let cache_material = Self::jit_cache_material(&ir, self.opt_level)?;
        let cache_key = Self::stable_cache_hash(&cache_material);
        let cache_library_path =
            cache_dir.join(format!("{cache_key}{}", std::env::consts::DLL_SUFFIX));
        let cache_key_path = cache_dir.join(format!("{cache_key}.key"));
        let cached_key = match fs::read(&cache_key_path) {
            Ok(key) => Some(key),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!("Failed to read JIT cache key: {}", error));
            }
        };

        if cached_key.as_deref() == Some(cache_material.as_bytes()) && cache_library_path.is_file()
        {
            if let Ok((library, entry_fn, function_ptrs)) =
                Self::load_compiled_library(&cache_library_path, module)
            {
                return Ok(CompiledJit {
                    library: Some(library),
                    _artifacts: JitArtifacts {
                        ir_path: None,
                        library_path: None,
                        import_library_path: None,
                    },
                    entry_fn,
                    function_ptrs,
                    globals_count: module.globals.len(),
                    constants: Self::all_module_constants(module),
                });
            }

            fs::remove_file(&cache_library_path)
                .map_err(|e| format!("Failed to remove invalid cached JIT library: {}", e))?;
            if cache_key_path.exists() {
                fs::remove_file(&cache_key_path)
                    .map_err(|e| format!("Failed to remove invalid JIT cache key: {}", e))?;
            }
        } else if cached_key.is_some() || cache_library_path.exists() {
            if cache_library_path.exists() {
                fs::remove_file(&cache_library_path)
                    .map_err(|e| format!("Failed to replace stale JIT cache library: {}", e))?;
            }
            if cache_key_path.exists() {
                fs::remove_file(&cache_key_path)
                    .map_err(|e| format!("Failed to replace stale JIT cache key: {}", e))?;
            }
        }

        let ir_path = temp_dir.join(format!("{base_name}.ll"));
        let library_path = temp_dir.join(format!(
            "{}{}{}",
            std::env::consts::DLL_PREFIX,
            base_name,
            std::env::consts::DLL_SUFFIX
        ));
        #[cfg(target_os = "windows")]
        let import_library_path = Some(temp_dir.join(format!("{base_name}.lib")));
        #[cfg(not(target_os = "windows"))]
        let import_library_path = None;
        let artifacts = JitArtifacts {
            ir_path: Some(ir_path.clone()),
            library_path: Some(library_path.clone()),
            import_library_path: import_library_path.clone(),
        };

        fs::write(&ir_path, &ir)
            .map_err(|e| format!("Failed to write LLVM IR temporary file: {}", e))?;

        let opt_flag = format!("-O{}", self.opt_level);
        let mut clang = Command::new("clang");
        clang.arg(&opt_flag);
        configure_shared_library_command(&mut clang, &library_path, &ir_path, module);
        let output = clang
            .output()
            .map_err(|e| format!("Failed to execute clang: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "LLVM JIT compilation failed via clang:\n{}",
                stderr
            ));
        }

        let (library, entry_fn, function_ptrs) =
            Self::load_compiled_library(&library_path, module)?;
        fs::copy(&library_path, &cache_library_path)
            .map_err(|e| format!("Failed to write compiled JIT cache library: {}", e))?;
        fs::write(&cache_key_path, cache_material.as_bytes())
            .map_err(|e| format!("Failed to write compiled JIT cache key: {}", e))?;

        Ok(CompiledJit {
            library: Some(library),
            _artifacts: artifacts,
            entry_fn,
            function_ptrs,
            globals_count: module.globals.len(),
            constants: Self::all_module_constants(module),
        })
    }

    fn all_module_constants(module: &SsaModule) -> Vec<crate::vm::Value> {
        let mut constants = module.constants.clone();
        for function in &module.functions {
            constants.extend(function.constants.iter().cloned());
        }
        constants
    }

    fn jit_cache_material(ir: &str, opt_level: u32) -> Result<String, String> {
        let fingerprint = CLANG_FINGERPRINT
            .get_or_init(|| {
                let version = Command::new("clang")
                    .arg("--version")
                    .output()
                    .map_err(|e| format!("Failed to identify clang for JIT cache: {}", e))?;
                if !version.status.success() {
                    return Err(format!(
                        "Failed to identify clang for JIT cache: {}",
                        String::from_utf8_lossy(&version.stderr)
                    ));
                }
                let target = Command::new("clang")
                    .arg("-dumpmachine")
                    .output()
                    .map_err(|e| format!("Failed to identify clang target for JIT cache: {}", e))?;
                if !target.status.success() {
                    return Err(format!(
                        "Failed to identify clang target for JIT cache: {}",
                        String::from_utf8_lossy(&target.stderr)
                    ));
                }
                Ok(format!(
                    "clang={}\ntarget={}\n",
                    String::from_utf8_lossy(&version.stdout),
                    String::from_utf8_lossy(&target.stdout)
                ))
            })
            .clone()?;
        Ok(format!(
            "kria-jit-cache-v1\nopt={}\nos={}\narch={}\n{}ir:\n{}",
            opt_level,
            std::env::consts::OS,
            std::env::consts::ARCH,
            fingerprint,
            ir
        ))
    }

    fn stable_cache_hash(material: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        material.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn load_compiled_library(
        library_path: &std::path::Path,
        module: &SsaModule,
    ) -> Result<
        (
            Library,
            extern "C" fn(*mut KriaContext, *const KriaRuntime),
            Vec<runtime::JitFn>,
        ),
        String,
    > {
        let library = unsafe { Library::new(library_path) }
            .map_err(|e| format!("Failed to load JIT shared library: {}", e))?;
        let mut function_ptrs = Vec::with_capacity(module.functions.len());
        for i in 0..module.functions.len() {
            let symbol_name = format!("kria_func_{}\0", i);
            let typed_fn = unsafe {
                *library
                    .get::<runtime::JitFn>(symbol_name.as_bytes())
                    .map_err(|e| format!("Failed to resolve symbol kria_func_{}: {}", i, e))?
            };
            function_ptrs.push(typed_fn);
        }
        let entry_fn = unsafe {
            *library
                .get::<extern "C" fn(*mut KriaContext, *const KriaRuntime)>(b"kria_entry\0")
                .map_err(|e| format!("Failed to resolve symbol kria_entry: {}", e))?
        };
        Ok((library, entry_fn, function_ptrs))
    }

    pub fn execute(&mut self, module: &SsaModule) -> Result<(), String> {
        let compiled = self.compile(module)?;
        compiled.run()
    }
}

fn configure_shared_library_command(
    command: &mut Command,
    library_path: &std::path::Path,
    ir_path: &std::path::Path,
    _module: &SsaModule,
) {
    #[cfg(target_os = "windows")]
    {
        command.arg("-shared");
        for i in 0.._module.functions.len() {
            command.arg(format!("-Wl,/EXPORT:kria_func_{}", i));
        }
        command.arg("-Wl,/EXPORT:kria_entry");
    }

    #[cfg(target_os = "macos")]
    command.arg("-dynamiclib");

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command.arg("-shared").arg("-fPIC");
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    command.arg("-shared");

    command.arg(ir_path).arg("-o").arg(library_path);
}
