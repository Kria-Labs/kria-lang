pub mod compiler;
pub mod runtime;
pub mod value;

#[cfg(test)]
mod tests;

pub use compiler::LlvmCompiler;
pub use runtime::{KriaContext, KriaRuntime, RUNTIME_TABLE};
pub use value::LlvmValue;

use crate::register::SsaModule;
use std::ffi::{CStr, CString};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static JIT_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct LlvmJit {
    opt_level: u32,
}

pub struct CompiledJit {
    handle: *mut libc::c_void,
    ll_path: std::path::PathBuf,
    so_path: std::path::PathBuf,
    entry_fn: extern "C" fn(*mut KriaContext, *const KriaRuntime),
    function_ptrs: Vec<runtime::JitFn>,
    globals_count: usize,
    constants: Vec<crate::vm::Value>,
}

unsafe impl Send for CompiledJit {}
unsafe impl Sync for CompiledJit {}

impl CompiledJit {
    pub fn run(&self) -> Result<(), String> {
        let mut ctx = KriaContext::new(self.globals_count, &self.constants);
        ctx.function_ptrs = self.function_ptrs.clone();
        (self.entry_fn)(&mut ctx, &RUNTIME_TABLE);

        if let Some(err) = ctx.error {
            return Err(err);
        }
        Ok(())
    }
}

impl Drop for CompiledJit {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                libc::dlclose(self.handle);
            }
        }
        let _ = fs::remove_file(&self.ll_path);
        let _ = fs::remove_file(&self.so_path);
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
        let ll_path = temp_dir.join(format!("kria_jit_{}_{}.ll", pid, count));
        let so_path = temp_dir.join(format!("kria_jit_{}_{}.so", pid, count));

        fs::write(&ll_path, &ir)
            .map_err(|e| format!("Failed to write LLVM IR temporary file: {}", e))?;

        let opt_flag = format!("-O{}", self.opt_level);
        let output = Command::new("clang")
            .arg(&opt_flag)
            .arg("-shared")
            .arg("-fPIC")
            .arg(&ll_path)
            .arg("-o")
            .arg(&so_path)
            .output()
            .map_err(|e| format!("Failed to execute clang: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = fs::remove_file(&ll_path);
            let _ = fs::remove_file(&so_path);
            return Err(format!("LLVM JIT compilation failed via clang:\n{}", stderr));
        }

        let so_c_str = CString::new(so_path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {}", e))?;

        let handle = unsafe { libc::dlopen(so_c_str.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let err = unsafe {
                let err_ptr = libc::dlerror();
                if err_ptr.is_null() {
                    "Unknown dlopen error".to_string()
                } else {
                    CStr::from_ptr(err_ptr).to_string_lossy().to_string()
                }
            };
            let _ = fs::remove_file(&ll_path);
            let _ = fs::remove_file(&so_path);
            return Err(format!("Failed to load JIT shared library: {}", err));
        }

        let mut function_ptrs = Vec::with_capacity(module.functions.len());

        // Resolve function pointers for closures
        for i in 0..module.functions.len() {
            let sym_name = CString::new(format!("kria_func_{}", i)).unwrap();
            let fn_ptr = unsafe { libc::dlsym(handle, sym_name.as_ptr()) };
            if fn_ptr.is_null() {
                unsafe { libc::dlclose(handle) };
                let _ = fs::remove_file(&ll_path);
                let _ = fs::remove_file(&so_path);
                return Err(format!("Failed to resolve symbol kria_func_{}", i));
            }
            let typed_fn: runtime::JitFn = unsafe { std::mem::transmute(fn_ptr) };
            function_ptrs.push(typed_fn);
        }

        // Resolve entry point
        let entry_sym = CString::new("kria_entry").unwrap();
        let entry_ptr = unsafe { libc::dlsym(handle, entry_sym.as_ptr()) };
        if entry_ptr.is_null() {
            unsafe { libc::dlclose(handle) };
            let _ = fs::remove_file(&ll_path);
            let _ = fs::remove_file(&so_path);
            return Err("Failed to resolve symbol kria_entry".to_string());
        }

        type EntryFn = extern "C" fn(*mut KriaContext, *const KriaRuntime);
        let entry_fn: EntryFn = unsafe { std::mem::transmute(entry_ptr) };

        Ok(CompiledJit {
            handle,
            ll_path,
            so_path,
            entry_fn,
            function_ptrs,
            globals_count: module.globals.len(),
            constants: module.constants.clone(),
        })
    }

    pub fn execute(&mut self, module: &SsaModule) -> Result<(), String> {
        let compiled = self.compile(module)?;
        compiled.run()
    }
}
