use crate::ast::BinaryOperator;
use crate::register::{
    CaptureKind, SsaBlock, SsaFunction, SsaInstruction, SsaModule, SsaTerminator,
};
use crate::vm::Value;
use std::collections::HashMap;

struct FuncBuffers {
    max_call_args: usize,
    max_closure_caps: usize,
    max_array_elems: usize,
    max_obj_fields: usize,
}

pub struct LlvmCompiler<'a> {
    module: &'a SsaModule,
    out: String,
    strings: Vec<String>,
    string_map: HashMap<String, usize>,
    temp_counter: usize,
}

impl<'a> LlvmCompiler<'a> {
    pub fn new(module: &'a SsaModule) -> Self {
        Self {
            module,
            out: String::with_capacity(16384),
            strings: Vec::new(),
            string_map: HashMap::new(),
            temp_counter: 0,
        }
    }

    fn fresh_id(&mut self) -> usize {
        let id = self.temp_counter;
        self.temp_counter += 1;
        id
    }

    fn intern_string(&mut self, s: &str) -> usize {
        if let Some(&idx) = self.string_map.get(s) {
            idx
        } else {
            let idx = self.strings.len();
            self.strings.push(s.to_string());
            self.string_map.insert(s.to_string(), idx);
            idx
        }
    }

    pub fn compile(mut self) -> String {
        let mut body = String::with_capacity(16384);

        // Pre-intern constant strings from the module
        for func in &self.module.functions {
            self.collect_strings_from_func(func);
        }
        self.collect_strings_from_func(&self.module.entry);

        // Compile functions
        for (i, func) in self.module.functions.iter().enumerate() {
            body.push_str(&self.compile_function(func, i));
        }

        // Compile entry
        body.push_str(&self.compile_entry(&self.module.entry));

        // Header
        self.emit_header();

        // String constants
        self.emit_string_constants();

        self.out.push_str(&body);
        #[cfg(target_os = "windows")]
        {
            lower_windows_runtime_abi(&self.out)
        }
        #[cfg(not(target_os = "windows"))]
        self.out
    }

    fn collect_strings_from_func(&mut self, func: &SsaFunction) {
        for c in &func.constants {
            if let Value::String(s) = c {
                self.intern_string(s.as_str());
            }
        }
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    SsaInstruction::PropertyGet { member, .. }
                    | SsaInstruction::PropertySet { member, .. }
                    | SsaInstruction::ObjectDelete { member, .. } => {
                        self.intern_string(member);
                    }
                    SsaInstruction::AppendStringGlobal { suffix, .. } => {
                        self.intern_string(suffix);
                    }
                    SsaInstruction::BuildObject { fields, .. } => {
                        for (k, _) in fields {
                            self.intern_string(k);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn emit_header(&mut self) {
        self.out.push_str(
            r#"%Value = type { i64, i64 }
%Context = type { %Value*, i64, %Value* }
%Runtime = type {
    void (%Context*, %Value)*,
    %Value (%Context*, %Value, i8)*,
    %Value (%Context*, i32, %Value, %Value)*,
    %Value (%Context*, i32, %Value)*,
    %Value (%Context*, %Value*, i32, i32)*,
    %Value (%Context*, i8**, i32*, %Value*, i32)*,
    %Value (%Context*, %Value, %Value)*,
    void (%Context*, %Value, %Value, %Value*)*,
    %Value (%Context*, %Value, i8*, i32)*,
    void (%Context*, %Value, i8*, i32, %Value*)*,
    %Value (%Context*, %Value)*,
    void (%Context*, %Value, %Value*)*,
    %Value (%Context*, %Value)*,
    %Value (%Context*, %Value)*,
    void (%Context*, %Value, i8*, i32)*,
    void (%Context*, %Value, %Value)*,
    %Value (%Context*, %Value)*,
    %Value (%Context*, %Value)*,
    %Value (%Context*, %Value)*,
    %Value (%Context*, %Value)*,
    %Value (%Context*, %Value)*,
    void (%Context*, i32, i8*, i32)*,
    %Value (%Context*, i32, %Value*, i8*, i32, i64)*,
    %Value (%Context*, %Runtime*, %Value, %Value*, i32)*,
    %Value (%Context*, i64, i32)*,
    void (%Context*, i64, i32, %Value*)*,
    void (%Context*, i8*, i32)*,
    i32 (%Context*)*
}

define private i1 @kria_is_truthy(%Value %val) {
entry:
    %tag = extractvalue %Value %val, 0
    %payload = extractvalue %Value %val, 1
    %is_bool = icmp eq i64 %tag, 3
    br i1 %is_bool, label %check_bool, label %check_null
check_bool:
    %b_res = icmp ne i64 %payload, 0
    ret i1 %b_res
check_null:
    %is_null = icmp eq i64 %tag, 0
    br i1 %is_null, label %ret_false, label %check_int
check_int:
    %is_int = icmp eq i64 %tag, 1
    br i1 %is_int, label %check_int_val, label %ret_true
check_int_val:
    %i_res = icmp ne i64 %payload, 0
    ret i1 %i_res
ret_false:
    ret i1 false
ret_true:
    ret i1 true
}

"#,
        );
    }

    fn emit_string_constants(&mut self) {
        for (i, s) in self.strings.iter().enumerate() {
            let escaped = escape_llvm_string(s.as_bytes());
            let len = s.len();
            self.out.push_str(&format!(
                "@kria_str_{} = private unnamed_addr constant [{} x i8] c\"{}\", align 1\n",
                i, len, escaped
            ));
        }
        self.out.push('\n');
    }

    fn compile_function(&mut self, func: &SsaFunction, func_idx: usize) -> String {
        let mut s = String::new();
        let param_count = func.params.len();

        // 1. Direct function
        let mut param_sigs = Vec::new();
        for i in 0..param_count {
            param_sigs.push(format!("%Value %param_{}", i));
        }
        let param_str = if param_sigs.is_empty() {
            String::new()
        } else {
            format!(", {}", param_sigs.join(", "))
        };

        s.push_str(&format!(
            "define %Value @kria_func_direct_{}(%Context* %ctx, %Runtime* %rt, i64 %captures_id{}) {{\nentry:\n",
            func_idx, param_str
        ));

        s.push_str(&self.emit_function_allocas_and_body(func, param_count, false));
        s.push_str("}\n\n");

        // 2. Uniform wrapper
        s.push_str(&format!(
            "define %Value @kria_func_{}(%Context* %ctx, %Runtime* %rt, i64 %captures_id, %Value* %args, i32 %num_args) {{\nentry:\n",
            func_idx
        ));

        let mut direct_args = Vec::new();
        for i in 0..param_count {
            s.push_str(&format!(
                "  %arg_ptr_{} = getelementptr %Value, %Value* %args, i32 {}\n",
                i, i
            ));
            s.push_str(&format!(
                "  %arg_{} = load %Value, %Value* %arg_ptr_{}\n",
                i, i
            ));
            direct_args.push(format!("%Value %arg_{}", i));
        }

        let direct_args_str = if direct_args.is_empty() {
            String::new()
        } else {
            format!(", {}", direct_args.join(", "))
        };

        s.push_str(&format!(
            "  %res = call %Value @kria_func_direct_{}(%Context* %ctx, %Runtime* %rt, i64 %captures_id{})\n",
            func_idx, direct_args_str
        ));
        s.push_str("  ret %Value %res\n}\n\n");

        s
    }

    fn compile_entry(&mut self, entry_func: &SsaFunction) -> String {
        let mut s = String::new();
        s.push_str("define void @kria_entry(%Context* %ctx, %Runtime* %rt) {\nentry:\n");
        s.push_str(&self.emit_function_allocas_and_body(entry_func, 0, true));
        s.push_str("}\n\n");
        s
    }

    fn emit_function_allocas_and_body(
        &mut self,
        func: &SsaFunction,
        param_count: usize,
        is_entry: bool,
    ) -> String {
        let mut s = String::new();

        // Allocas for registers
        for i in 0..func.max_registers {
            s.push_str(&format!("  %reg_{} = alloca %Value\n", i));
            s.push_str(&format!(
                "  store %Value {{ i64 0, i64 0 }}, %Value* %reg_{}\n",
                i
            ));
        }

        // Allocas for locals
        for i in 0..func.local_count {
            s.push_str(&format!("  %local_{} = alloca %Value\n", i));
            if (i as usize) < param_count {
                s.push_str(&format!(
                    "  store %Value %param_{}, %Value* %local_{}\n",
                    i, i
                ));
            } else {
                s.push_str(&format!(
                    "  store %Value {{ i64 0, i64 0 }}, %Value* %local_{}\n",
                    i
                ));
            }
        }

        let mut max_call_args = 1;
        let mut max_closure_caps = 1;
        let mut max_array_elems = 1;
        let mut max_obj_fields = 1;

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    SsaInstruction::Call { args, .. } => {
                        max_call_args = max_call_args.max(args.len());
                    }
                    SsaInstruction::MakeClosure { captures, .. } => {
                        max_closure_caps = max_closure_caps.max(captures.len());
                    }
                    SsaInstruction::BuildArray { elements, .. } => {
                        max_array_elems = max_array_elems.max(elements.len());
                    }
                    SsaInstruction::BuildObject { fields, .. } => {
                        max_obj_fields = max_obj_fields.max(fields.len());
                    }
                    _ => {}
                }
            }
        }

        s.push_str(&format!(
            "  %call_args_buf = alloca [{} x %Value]\n",
            max_call_args
        ));
        s.push_str(&format!(
            "  %mc_caps_buf = alloca [{} x %Value]\n",
            max_closure_caps
        ));
        s.push_str(&format!(
            "  %mc_kinds_buf = alloca [{} x i8]\n",
            max_closure_caps
        ));
        s.push_str(&format!(
            "  %ba_buf = alloca [{} x %Value]\n",
            max_array_elems
        ));
        s.push_str(&format!(
            "  %bo_k_buf = alloca [{} x i8*]\n",
            max_obj_fields
        ));
        s.push_str(&format!(
            "  %bo_l_buf = alloca [{} x i32]\n",
            max_obj_fields
        ));
        s.push_str(&format!(
            "  %bo_v_buf = alloca [{} x %Value]\n",
            max_obj_fields
        ));

        s.push_str("  br label %block_0\n\n");

        let bufs = FuncBuffers {
            max_call_args,
            max_closure_caps,
            max_array_elems,
            max_obj_fields,
        };

        // Emit blocks
        for (bi, block) in func.blocks.iter().enumerate() {
            s.push_str(&format!("block_{}:\n", block.id.0));
            s.push_str(&self.emit_block_instructions(func, block, is_entry, &bufs));

            // Check if block ends with a terminator
            let next_block = func.blocks.get(bi + 1).map(|b| b.id.0);
            s.push_str(&self.emit_block_terminator(&block.terminator, next_block, is_entry));
            s.push('\n');
        }

        s
    }

    fn emit_block_instructions(
        &mut self,
        func: &SsaFunction,
        block: &SsaBlock,
        is_entry: bool,
        bufs: &FuncBuffers,
    ) -> String {
        let mut s = String::new();
        let cap_id = if is_entry { "0" } else { "%captures_id" };

        for inst in &block.instructions {
            match inst {
                SsaInstruction::Const { dst, idx } => {
                    let constant = func.constants.get(*idx as usize);
                    match constant {
                        Some(Value::Number(n)) => {
                            s.push_str(&format!(
                                "  store %Value {{ i64 1, i64 {} }}, %Value* %reg_{}\n",
                                n, dst.0
                            ));
                        }
                        Some(Value::Float(f)) => {
                            s.push_str(&format!(
                                "  store %Value {{ i64 2, i64 {} }}, %Value* %reg_{}\n",
                                f.to_bits(),
                                dst.0
                            ));
                        }
                        Some(Value::Boolean(b)) => {
                            let b_val = if *b { 1 } else { 0 };
                            s.push_str(&format!(
                                "  store %Value {{ i64 3, i64 {} }}, %Value* %reg_{}\n",
                                b_val, dst.0
                            ));
                        }
                        Some(Value::Null) => {
                            s.push_str(&format!(
                                "  store %Value {{ i64 0, i64 0 }}, %Value* %reg_{}\n",
                                dst.0
                            ));
                        }
                        _ => {
                            // Load from ctx constants
                            let tmp = self.fresh_id();
                            s.push_str(&format!(
                                "  %c_ptr_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 2\n"
                            ));
                            s.push_str(&format!(
                                "  %c_ptr_{tmp} = load %Value*, %Value** %c_ptr_ptr_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %c_slot_{tmp} = getelementptr %Value, %Value* %c_ptr_{tmp}, i32 {}\n",
                                idx
                            ));
                            s.push_str(&format!(
                                "  %c_val_{tmp} = load %Value, %Value* %c_slot_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  store %Value %c_val_{tmp}, %Value* %reg_{}\n",
                                dst.0
                            ));
                        }
                    }
                }
                SsaInstruction::Null { dst } => {
                    s.push_str(&format!(
                        "  store %Value {{ i64 0, i64 0 }}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::LoadGlobal { dst, slot } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %g_ptr_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 0\n"
                    ));
                    s.push_str(&format!(
                        "  %g_ptr_{tmp} = load %Value*, %Value** %g_ptr_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %g_slot_{tmp} = getelementptr %Value, %Value* %g_ptr_{tmp}, i32 {}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  %g_val_{tmp} = load %Value, %Value* %g_slot_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %g_val_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::StoreGlobal { slot, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %g_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  %g_ptr_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 0\n"
                    ));
                    s.push_str(&format!(
                        "  %g_ptr_{tmp} = load %Value*, %Value** %g_ptr_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %g_slot_{tmp} = getelementptr %Value, %Value* %g_ptr_{tmp}, i32 {}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  store %Value %g_v_{tmp}, %Value* %g_slot_{tmp}\n"
                    ));
                }
                SsaInstruction::LoadLocal { dst, slot } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %loc_{tmp} = load %Value, %Value* %local_{}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  store %Value %loc_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::StoreLocal { slot, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %loc_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  store %Value %loc_v_{tmp}, %Value* %local_{}\n",
                        slot
                    ));
                }
                SsaInstruction::Move { dst, src } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %mov_{tmp} = load %Value, %Value* %reg_{}\n",
                        src.0
                    ));
                    s.push_str(&format!(
                        "  store %Value %mov_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::Unary { dst, op, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %u_val_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    if matches!(op, crate::ast::UnaryOperator::Not) {
                        s.push_str(&format!(
                            "  %u_truthy_{tmp} = call i1 @kria_is_truthy(%Value %u_val_{tmp})\n"
                        ));
                        s.push_str(&format!("  %u_not_{tmp} = xor i1 %u_truthy_{tmp}, true\n"));
                        s.push_str(&format!("  %u_ext_{tmp} = zext i1 %u_not_{tmp} to i64\n"));
                        s.push_str(&format!(
                            "  store %Value {{ i64 3, i64 0 }}, %Value* %reg_{}\n",
                            dst.0
                        ));
                        s.push_str(&format!(
                            "  %u_res_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %u_ext_{tmp}, 1\n"
                        ));
                        s.push_str(&format!(
                            "  store %Value %u_res_{tmp}, %Value* %reg_{}\n",
                            dst.0
                        ));
                    }
                }
                SsaInstruction::BinaryInt {
                    dst,
                    op,
                    left,
                    right,
                } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %bi_l_{tmp} = load %Value, %Value* %reg_{}\n",
                        left.0
                    ));
                    s.push_str(&format!(
                        "  %bi_r_{tmp} = load %Value, %Value* %reg_{}\n",
                        right.0
                    ));
                    s.push_str(&format!(
                        "  %bi_lnum_{tmp} = extractvalue %Value %bi_l_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  %bi_rnum_{tmp} = extractvalue %Value %bi_r_{tmp}, 1\n"
                    ));

                    match op {
                        BinaryOperator::Add => {
                            s.push_str(&format!(
                                "  %bi_res_{tmp} = add i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %bi_res_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::Subtract => {
                            s.push_str(&format!(
                                "  %bi_res_{tmp} = sub i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %bi_res_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::Multiply => {
                            s.push_str(&format!(
                                "  %bi_res_{tmp} = mul i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %bi_res_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::Divide => {
                            s.push_str(&format!(
                                "  %bi_res_{tmp} = sdiv i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %bi_res_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::Equals => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp eq i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::NotEquals => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp ne i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::GreaterThan => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp sgt i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::LessThan => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp slt i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::GreaterOrEqual => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp sge i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::LessOrEqual => {
                            s.push_str(&format!(
                                "  %bi_cmp_{tmp} = icmp sle i64 %bi_lnum_{tmp}, %bi_rnum_{tmp}\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_ext_{tmp} = zext i1 %bi_cmp_{tmp} to i64\n"
                            ));
                            s.push_str(&format!(
                                "  %bi_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %bi_ext_{tmp}, 1\n"
                            ));
                        }
                        BinaryOperator::And | BinaryOperator::Or => {
                            s.push_str(&format!("  %bi_val_{tmp} = {{ i64 0, i64 0 }}\n"));
                        }
                    }
                    s.push_str(&format!(
                        "  store %Value %bi_val_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::Binary {
                    dst,
                    op,
                    left,
                    right,
                } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %b_l_{tmp} = load %Value, %Value* %reg_{}\n",
                        left.0
                    ));
                    s.push_str(&format!(
                        "  %b_r_{tmp} = load %Value, %Value* %reg_{}\n",
                        right.0
                    ));

                    if matches!(op, BinaryOperator::And) {
                        s.push_str(&format!(
                            "  %b_lt_{tmp} = call i1 @kria_is_truthy(%Value %b_l_{tmp})\n"
                        ));
                        s.push_str(&format!(
                            "  %b_rt_{tmp} = call i1 @kria_is_truthy(%Value %b_r_{tmp})\n"
                        ));
                        s.push_str(&format!(
                            "  %b_and_{tmp} = and i1 %b_lt_{tmp}, %b_rt_{tmp}\n"
                        ));
                        s.push_str(&format!("  %b_ext_{tmp} = zext i1 %b_and_{tmp} to i64\n"));
                        s.push_str(&format!(
                            "  %b_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_ext_{tmp}, 1\n"
                        ));
                        s.push_str(&format!(
                            "  store %Value %b_val_{tmp}, %Value* %reg_{}\n",
                            dst.0
                        ));
                    } else if matches!(op, BinaryOperator::Or) {
                        s.push_str(&format!(
                            "  %b_lt_{tmp} = call i1 @kria_is_truthy(%Value %b_l_{tmp})\n"
                        ));
                        s.push_str(&format!(
                            "  %b_rt_{tmp} = call i1 @kria_is_truthy(%Value %b_r_{tmp})\n"
                        ));
                        s.push_str(&format!("  %b_or_{tmp} = or i1 %b_lt_{tmp}, %b_rt_{tmp}\n"));
                        s.push_str(&format!("  %b_ext_{tmp} = zext i1 %b_or_{tmp} to i64\n"));
                        s.push_str(&format!(
                            "  %b_val_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_ext_{tmp}, 1\n"
                        ));
                        s.push_str(&format!(
                            "  store %Value %b_val_{tmp}, %Value* %reg_{}\n",
                            dst.0
                        ));
                    } else {
                        // Check if both operands are integers (tag == 1)
                        s.push_str(&format!(
                            "  %b_lt_{tmp} = extractvalue %Value %b_l_{tmp}, 0\n"
                        ));
                        s.push_str(&format!(
                            "  %b_rt_{tmp} = extractvalue %Value %b_r_{tmp}, 0\n"
                        ));
                        s.push_str(&format!("  %b_li_{tmp} = icmp eq i64 %b_lt_{tmp}, 1\n"));
                        s.push_str(&format!("  %b_ri_{tmp} = icmp eq i64 %b_rt_{tmp}, 1\n"));
                        s.push_str(&format!(
                            "  %b_bi_{tmp} = and i1 %b_li_{tmp}, %b_ri_{tmp}\n"
                        ));

                        let cond_name = if matches!(op, BinaryOperator::Divide) {
                            s.push_str(&format!(
                                "  %b_rn_{tmp} = extractvalue %Value %b_r_{tmp}, 1\n"
                            ));
                            s.push_str(&format!("  %b_rnz_{tmp} = icmp ne i64 %b_rn_{tmp}, 0\n"));
                            s.push_str(&format!(
                                "  %b_cond_{tmp} = and i1 %b_bi_{tmp}, %b_rnz_{tmp}\n"
                            ));
                            format!("%b_cond_{tmp}")
                        } else {
                            format!("%b_bi_{tmp}")
                        };

                        s.push_str(&format!(
                            "  br i1 {}, label %b_fast_{tmp}, label %b_slow_{tmp}\n\n",
                            cond_name
                        ));

                        // Fast integer path
                        s.push_str(&format!("b_fast_{tmp}:\n"));
                        s.push_str(&format!(
                            "  %b_fl_{tmp} = extractvalue %Value %b_l_{tmp}, 1\n"
                        ));
                        s.push_str(&format!(
                            "  %b_fr_{tmp} = extractvalue %Value %b_r_{tmp}, 1\n"
                        ));

                        match op {
                            BinaryOperator::Add => {
                                s.push_str(&format!(
                                    "  %b_fres_{tmp} = add i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %b_fres_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::Subtract => {
                                s.push_str(&format!(
                                    "  %b_fres_{tmp} = sub i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %b_fres_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::Multiply => {
                                s.push_str(&format!(
                                    "  %b_fres_{tmp} = mul i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %b_fres_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::Divide => {
                                s.push_str(&format!(
                                    "  %b_fres_{tmp} = sdiv i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %b_fres_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::Equals => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp eq i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::NotEquals => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp ne i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::GreaterThan => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp sgt i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::LessThan => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp slt i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::GreaterOrEqual => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp sge i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::LessOrEqual => {
                                s.push_str(&format!(
                                    "  %b_fcmp_{tmp} = icmp sle i64 %b_fl_{tmp}, %b_fr_{tmp}\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fext_{tmp} = zext i1 %b_fcmp_{tmp} to i64\n"
                                ));
                                s.push_str(&format!(
                                    "  %b_fval_{tmp} = insertvalue %Value {{ i64 3, i64 0 }}, i64 %b_fext_{tmp}, 1\n"
                                ));
                            }
                            BinaryOperator::And | BinaryOperator::Or => unreachable!(),
                        }

                        s.push_str(&format!(
                            "  store %Value %b_fval_{tmp}, %Value* %reg_{}\n",
                            dst.0
                        ));
                        s.push_str(&format!("  br label %b_cont_{tmp}\n\n"));

                        // Slow fallback path
                        s.push_str(&format!("b_slow_{tmp}:\n"));
                        let op_code = match op {
                            BinaryOperator::Add => 0,
                            BinaryOperator::Subtract => 1,
                            BinaryOperator::Multiply => 2,
                            BinaryOperator::Divide => 3,
                            BinaryOperator::Equals => 4,
                            BinaryOperator::NotEquals => 5,
                            BinaryOperator::GreaterThan => 6,
                            BinaryOperator::LessThan => 7,
                            BinaryOperator::GreaterOrEqual => 8,
                            BinaryOperator::LessOrEqual => 9,
                            BinaryOperator::And => 10,
                            BinaryOperator::Or => 11,
                        };
                        s.push_str(&format!(
                            "  %b_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 2\n"
                        ));
                        s.push_str(&format!(
                            "  %b_fn_{tmp} = load %Value (%Context*, i32, %Value, %Value)*, %Value (%Context*, i32, %Value, %Value)** %b_fn_ptr_{tmp}\n"
                        ));
                        s.push_str(&format!(
                            "  %b_res_{tmp} = call %Value %b_fn_{tmp}(%Context* %ctx, i32 {}, %Value %b_l_{tmp}, %Value %b_r_{tmp})\n",
                            op_code
                        ));
                        s.push_str(&format!(
                            "  store %Value %b_res_{tmp}, %Value* %reg_{}\n",
                            dst.0
                        ));
                        s.push_str(&format!("  br label %b_cont_{tmp}\n\n"));

                        // Continuation block
                        s.push_str(&format!("b_cont_{tmp}:\n"));
                    }
                }
                SsaInstruction::Print { value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %p_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 0\n"
                    ));
                    s.push_str(&format!(
                        "  %p_fn_{tmp} = load void (%Context*, %Value)*, void (%Context*, %Value)** %p_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %p_val_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  call void %p_fn_{tmp}(%Context* %ctx, %Value %p_val_{tmp})\n"
                    ));
                }
                SsaInstruction::Input {
                    dst,
                    prompt,
                    type_mask,
                } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %in_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 1\n"
                    ));
                    s.push_str(&format!(
                        "  %in_fn_{tmp} = load %Value (%Context*, %Value, i8)*, %Value (%Context*, %Value, i8)** %in_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %in_pr_{tmp} = load %Value, %Value* %reg_{}\n",
                        prompt.0
                    ));
                    s.push_str(&format!(
                        "  %in_res_{tmp} = call %Value %in_fn_{tmp}(%Context* %ctx, %Value %in_pr_{tmp}, i8 {})\n",
                        type_mask
                    ));
                    s.push_str(&format!(
                        "  store %Value %in_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::CallDirect {
                    dst,
                    function_index,
                    args,
                } => {
                    let tmp = self.fresh_id();
                    let mut arg_strs = Vec::new();
                    for (ai, arg) in args.iter().enumerate() {
                        s.push_str(&format!(
                            "  %cd_a_{}_{} = load %Value, %Value* %reg_{}\n",
                            tmp, ai, arg.0
                        ));
                        arg_strs.push(format!("%Value %cd_a_{}_{}", tmp, ai));
                    }
                    let args_joined = if arg_strs.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", arg_strs.join(", "))
                    };
                    s.push_str(&format!(
                        "  %cd_res_{tmp} = call %Value @kria_func_direct_{}(%Context* %ctx, %Runtime* %rt, i64 0{})\n",
                        function_index, args_joined
                    ));
                    s.push_str(&format!(
                        "  store %Value %cd_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::Call { dst, callee, args } => {
                    let tmp = self.fresh_id();
                    let num_args = args.len();
                    for (ai, arg) in args.iter().enumerate() {
                        s.push_str(&format!(
                            "  %c_a_{}_{} = load %Value, %Value* %reg_{}\n",
                            tmp, ai, arg.0
                        ));
                        s.push_str(&format!(
                            "  %c_sl_{}_{} = getelementptr [{} x %Value], [{} x %Value]* %call_args_buf, i32 0, i32 {}\n",
                            tmp, ai, bufs.max_call_args, bufs.max_call_args, ai
                        ));
                        s.push_str(&format!(
                            "  store %Value %c_a_{}_{}, %Value* %c_sl_{}_{}\n",
                            tmp, ai, tmp, ai
                        ));
                    }
                    s.push_str(&format!(
                        "  %c_callee_{tmp} = load %Value, %Value* %reg_{}\n",
                        callee.0
                    ));
                    s.push_str(&format!(
                        "  %c_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 23\n"
                    ));
                    s.push_str(&format!(
                        "  %c_fn_{tmp} = load %Value (%Context*, %Runtime*, %Value, %Value*, i32)*, %Value (%Context*, %Runtime*, %Value, %Value*, i32)** %c_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %c_raw_args_{tmp} = getelementptr [{} x %Value], [{} x %Value]* %call_args_buf, i32 0, i32 0\n",
                        bufs.max_call_args, bufs.max_call_args
                    ));
                    s.push_str(&format!(
                        "  %c_res_{tmp} = call %Value %c_fn_{tmp}(%Context* %ctx, %Runtime* %rt, %Value %c_callee_{tmp}, %Value* %c_raw_args_{tmp}, i32 {})\n",
                        num_args
                    ));
                    s.push_str(&format!(
                        "  store %Value %c_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::MakeClosure {
                    dst,
                    function,
                    captures,
                } => {
                    let tmp = self.fresh_id();
                    let count = captures.len();
                    for (ci, cap) in captures.iter().enumerate() {
                        let (kind_val, llvm_val_str) = match cap {
                            CaptureKind::Local(slot) => {
                                let local_tmp = self.fresh_id();
                                s.push_str(&format!(
                                    "  %mc_loc_{local_tmp} = load %Value, %Value* %local_{}\n",
                                    slot
                                ));
                                (0u8, format!("%Value %mc_loc_{local_tmp}"))
                            }
                            CaptureKind::Upvalue(slot) => {
                                (1u8, format!("%Value {{ i64 0, i64 {} }}", slot))
                            }
                        };
                        s.push_str(&format!(
                            "  %mc_c_sl_{}_{} = getelementptr [{} x %Value], [{} x %Value]* %mc_caps_buf, i32 0, i32 {}\n",
                            tmp, ci, bufs.max_closure_caps, bufs.max_closure_caps, ci
                        ));
                        s.push_str(&format!(
                            "  store {} , %Value* %mc_c_sl_{}_{}\n",
                            llvm_val_str, tmp, ci
                        ));
                        s.push_str(&format!(
                            "  %mc_k_sl_{}_{} = getelementptr [{} x i8], [{} x i8]* %mc_kinds_buf, i32 0, i32 {}\n",
                            tmp, ci, bufs.max_closure_caps, bufs.max_closure_caps, ci
                        ));
                        s.push_str(&format!(
                            "  store i8 {}, i8* %mc_k_sl_{}_{}\n",
                            kind_val, tmp, ci
                        ));
                    }
                    s.push_str(&format!(
                        "  %mc_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 22\n"
                    ));
                    s.push_str(&format!(
                        "  %mc_fn_{tmp} = load %Value (%Context*, i32, %Value*, i8*, i32, i64)*, %Value (%Context*, i32, %Value*, i8*, i32, i64)** %mc_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %mc_raw_c_{tmp} = getelementptr [{} x %Value], [{} x %Value]* %mc_caps_buf, i32 0, i32 0\n",
                        bufs.max_closure_caps, bufs.max_closure_caps
                    ));
                    s.push_str(&format!(
                        "  %mc_raw_k_{tmp} = getelementptr [{} x i8], [{} x i8]* %mc_kinds_buf, i32 0, i32 0\n",
                        bufs.max_closure_caps, bufs.max_closure_caps
                    ));
                    s.push_str(&format!(
                        "  %mc_res_{tmp} = call %Value %mc_fn_{tmp}(%Context* %ctx, i32 {}, %Value* %mc_raw_c_{tmp}, i8* %mc_raw_k_{tmp}, i32 {}, i64 {})\n",
                        function, count, cap_id
                    ));
                    s.push_str(&format!(
                        "  store %Value %mc_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::LoadUpvalue { dst, slot } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %lu_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 24\n"
                    ));
                    s.push_str(&format!(
                        "  %lu_fn_{tmp} = load %Value (%Context*, i64, i32)*, %Value (%Context*, i64, i32)** %lu_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %lu_res_{tmp} = call %Value %lu_fn_{tmp}(%Context* %ctx, i64 {}, i32 {})\n",
                        cap_id, slot
                    ));
                    s.push_str(&format!(
                        "  store %Value %lu_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::StoreUpvalue { slot, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %su_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 25\n"
                    ));
                    s.push_str(&format!(
                        "  %su_fn_{tmp} = load void (%Context*, i64, i32, %Value*)*, void (%Context*, i64, i32, %Value*)** %su_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  call void %su_fn_{tmp}(%Context* %ctx, i64 {}, i32 {}, %Value* %reg_{})\n",
                        cap_id, slot, value.0
                    ));
                }
                SsaInstruction::BuildArray {
                    dst,
                    elements,
                    mutable,
                } => {
                    let tmp = self.fresh_id();
                    let count = elements.len();
                    for (ei, elem) in elements.iter().enumerate() {
                        s.push_str(&format!(
                            "  %ba_e_{}_{} = load %Value, %Value* %reg_{}\n",
                            tmp, ei, elem.0
                        ));
                        s.push_str(&format!(
                            "  %ba_sl_{}_{} = getelementptr [{} x %Value], [{} x %Value]* %ba_buf, i32 0, i32 {}\n",
                            tmp, ei, bufs.max_array_elems, bufs.max_array_elems, ei
                        ));
                        s.push_str(&format!(
                            "  store %Value %ba_e_{}_{}, %Value* %ba_sl_{}_{}\n",
                            tmp, ei, tmp, ei
                        ));
                    }
                    s.push_str(&format!(
                        "  %ba_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 4\n"
                    ));
                    s.push_str(&format!(
                        "  %ba_fn_{tmp} = load %Value (%Context*, %Value*, i32, i32)*, %Value (%Context*, %Value*, i32, i32)** %ba_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ba_raw_{tmp} = getelementptr [{} x %Value], [{} x %Value]* %ba_buf, i32 0, i32 0\n",
                        bufs.max_array_elems, bufs.max_array_elems
                    ));
                    let mut_flag = if *mutable { 1 } else { 0 };
                    s.push_str(&format!(
                        "  %ba_res_{tmp} = call %Value %ba_fn_{tmp}(%Context* %ctx, %Value* %ba_raw_{tmp}, i32 {}, i32 {})\n",
                        count, mut_flag
                    ));
                    s.push_str(&format!(
                        "  store %Value %ba_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::BuildObject { dst, fields } => {
                    let tmp = self.fresh_id();
                    let count = fields.len();
                    for (fi, (k, val)) in fields.iter().enumerate() {
                        let str_idx = self.intern_string(k);
                        let k_len = k.len();
                        s.push_str(&format!(
                            "  %bo_k_sl_{fi}_{tmp} = getelementptr [{} x i8*], [{} x i8*]* %bo_k_buf, i32 0, i32 {}\n",
                            bufs.max_obj_fields, bufs.max_obj_fields, fi
                        ));
                        s.push_str(&format!(
                            "  %bo_str_raw_{fi}_{tmp} = getelementptr inbounds [{} x i8], [{} x i8]* @kria_str_{}, i32 0, i32 0\n",
                            k_len, k_len, str_idx
                        ));
                        s.push_str(&format!(
                            "  store i8* %bo_str_raw_{fi}_{tmp}, i8** %bo_k_sl_{fi}_{tmp}\n"
                        ));

                        s.push_str(&format!(
                            "  %bo_l_sl_{fi}_{tmp} = getelementptr [{} x i32], [{} x i32]* %bo_l_buf, i32 0, i32 {}\n",
                            bufs.max_obj_fields, bufs.max_obj_fields, fi
                        ));
                        s.push_str(&format!(
                            "  store i32 {}, i32* %bo_l_sl_{fi}_{tmp}\n",
                            k_len
                        ));

                        s.push_str(&format!(
                            "  %bo_fv_{fi}_{tmp} = load %Value, %Value* %reg_{}\n",
                            val.0
                        ));
                        s.push_str(&format!(
                            "  %bo_v_sl_{fi}_{tmp} = getelementptr [{} x %Value], [{} x %Value]* %bo_v_buf, i32 0, i32 {}\n",
                            bufs.max_obj_fields, bufs.max_obj_fields, fi
                        ));
                        s.push_str(&format!(
                            "  store %Value %bo_fv_{fi}_{tmp}, %Value* %bo_v_sl_{fi}_{tmp}\n"
                        ));
                    }
                    s.push_str(&format!(
                        "  %bo_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 5\n"
                    ));
                    s.push_str(&format!(
                        "  %bo_fn_{tmp} = load %Value (%Context*, i8**, i32*, %Value*, i32)*, %Value (%Context*, i8**, i32*, %Value*, i32)** %bo_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %bo_raw_k_{tmp} = getelementptr [{} x i8*], [{} x i8*]* %bo_k_buf, i32 0, i32 0\n",
                        bufs.max_obj_fields, bufs.max_obj_fields
                    ));
                    s.push_str(&format!(
                        "  %bo_raw_l_{tmp} = getelementptr [{} x i32], [{} x i32]* %bo_l_buf, i32 0, i32 0\n",
                        bufs.max_obj_fields, bufs.max_obj_fields
                    ));
                    s.push_str(&format!(
                        "  %bo_raw_v_{tmp} = getelementptr [{} x %Value], [{} x %Value]* %bo_v_buf, i32 0, i32 0\n",
                        bufs.max_obj_fields, bufs.max_obj_fields
                    ));
                    s.push_str(&format!(
                        "  %bo_res_{tmp} = call %Value %bo_fn_{tmp}(%Context* %ctx, i8** %bo_raw_k_{tmp}, i32* %bo_raw_l_{tmp}, %Value* %bo_raw_v_{tmp}, i32 {})\n",
                        count
                    ));
                    s.push_str(&format!(
                        "  store %Value %bo_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::IndexGet { dst, object, index } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ig_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %ig_i_{tmp} = load %Value, %Value* %reg_{}\n",
                        index.0
                    ));
                    s.push_str(&format!(
                        "  %ig_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 6\n"
                    ));
                    s.push_str(&format!(
                        "  %ig_fn_{tmp} = load %Value (%Context*, %Value, %Value)*, %Value (%Context*, %Value, %Value)** %ig_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ig_res_{tmp} = call %Value %ig_fn_{tmp}(%Context* %ctx, %Value %ig_o_{tmp}, %Value %ig_i_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %ig_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::IndexSet {
                    object,
                    index,
                    value,
                } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %is_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %is_i_{tmp} = load %Value, %Value* %reg_{}\n",
                        index.0
                    ));
                    s.push_str(&format!(
                        "  %is_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 7\n"
                    ));
                    s.push_str(&format!(
                        "  %is_fn_{tmp} = load void (%Context*, %Value, %Value, %Value*)*, void (%Context*, %Value, %Value, %Value*)** %is_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  call void %is_fn_{tmp}(%Context* %ctx, %Value %is_o_{tmp}, %Value %is_i_{tmp}, %Value* %reg_{})\n",
                        value.0
                    ));
                }
                SsaInstruction::PropertyGet {
                    dst,
                    object,
                    member,
                } => {
                    let tmp = self.fresh_id();
                    let str_idx = self.intern_string(member);
                    let len = member.len();
                    s.push_str(&format!(
                        "  %pg_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %pg_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 8\n"
                    ));
                    s.push_str(&format!(
                        "  %pg_fn_{tmp} = load %Value (%Context*, %Value, i8*, i32)*, %Value (%Context*, %Value, i8*, i32)** %pg_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %pg_m_raw_{tmp} = getelementptr inbounds [{} x i8], [{} x i8]* @kria_str_{}, i32 0, i32 0\n",
                        len, len, str_idx
                    ));
                    s.push_str(&format!(
                        "  %pg_res_{tmp} = call %Value %pg_fn_{tmp}(%Context* %ctx, %Value %pg_o_{tmp}, i8* %pg_m_raw_{tmp}, i32 {})\n",
                        len
                    ));
                    s.push_str(&format!(
                        "  store %Value %pg_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::PropertySet {
                    object,
                    member,
                    value,
                } => {
                    let tmp = self.fresh_id();
                    let str_idx = self.intern_string(member);
                    let len = member.len();
                    s.push_str(&format!(
                        "  %ps_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %ps_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 9\n"
                    ));
                    s.push_str(&format!(
                        "  %ps_fn_{tmp} = load void (%Context*, %Value, i8*, i32, %Value*)*, void (%Context*, %Value, i8*, i32, %Value*)** %ps_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ps_m_raw_{tmp} = getelementptr inbounds [{} x i8], [{} x i8]* @kria_str_{}, i32 0, i32 0\n",
                        len, len, str_idx
                    ));
                    s.push_str(&format!(
                        "  call void %ps_fn_{tmp}(%Context* %ctx, %Value %ps_o_{tmp}, i8* %ps_m_raw_{tmp}, i32 {}, %Value* %reg_{})\n",
                        len, value.0
                    ));
                }
                SsaInstruction::ArrayLen { dst, object } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %al_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %al_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 10\n"
                    ));
                    s.push_str(&format!(
                        "  %al_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %al_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %al_res_{tmp} = call %Value %al_fn_{tmp}(%Context* %ctx, %Value %al_o_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %al_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::ArrayPush { object, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ap_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %ap_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 11\n"
                    ));
                    s.push_str(&format!(
                        "  %ap_fn_{tmp} = load void (%Context*, %Value, %Value*)*, void (%Context*, %Value, %Value*)** %ap_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  call void %ap_fn_{tmp}(%Context* %ctx, %Value %ap_o_{tmp}, %Value* %reg_{})\n",
                        value.0
                    ));
                }
                SsaInstruction::ArrayPop { dst, object } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %pop_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %pop_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 12\n"
                    ));
                    s.push_str(&format!(
                        "  %pop_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %pop_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %pop_res_{tmp} = call %Value %pop_fn_{tmp}(%Context* %ctx, %Value %pop_o_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %pop_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::ObjectKeys { dst, object } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ok_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %ok_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 13\n"
                    ));
                    s.push_str(&format!(
                        "  %ok_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %ok_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ok_res_{tmp} = call %Value %ok_fn_{tmp}(%Context* %ctx, %Value %ok_o_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %ok_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::ObjectDelete { object, member } => {
                    let tmp = self.fresh_id();
                    let str_idx = self.intern_string(member);
                    let len = member.len();
                    s.push_str(&format!(
                        "  %od_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %od_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 14\n"
                    ));
                    s.push_str(&format!(
                        "  %od_fn_{tmp} = load void (%Context*, %Value, i8*, i32)*, void (%Context*, %Value, i8*, i32)** %od_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %od_m_raw_{tmp} = getelementptr inbounds [{} x i8], [{} x i8]* @kria_str_{}, i32 0, i32 0\n",
                        len, len, str_idx
                    ));
                    s.push_str(&format!(
                        "  call void %od_fn_{tmp}(%Context* %ctx, %Value %od_o_{tmp}, i8* %od_m_raw_{tmp}, i32 {})\n",
                        len
                    ));
                }
                SsaInstruction::ObjectDeleteKey { object, key } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %odk_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %odk_k_{tmp} = load %Value, %Value* %reg_{}\n",
                        key.0
                    ));
                    s.push_str(&format!(
                        "  %odk_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 15\n"
                    ));
                    s.push_str(&format!(
                        "  %odk_fn_{tmp} = load void (%Context*, %Value, %Value)*, void (%Context*, %Value, %Value)** %odk_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  call void %odk_fn_{tmp}(%Context* %ctx, %Value %odk_o_{tmp}, %Value %odk_k_{tmp})\n"
                    ));
                }
                SsaInstruction::IsArray { dst, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ia_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  %ia_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 16\n"
                    ));
                    s.push_str(&format!(
                        "  %ia_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %ia_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ia_res_{tmp} = call %Value %ia_fn_{tmp}(%Context* %ctx, %Value %ia_v_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %ia_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::IsObject { dst, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %io_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  %io_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 17\n"
                    ));
                    s.push_str(&format!(
                        "  %io_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %io_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %io_res_{tmp} = call %Value %io_fn_{tmp}(%Context* %ctx, %Value %io_v_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %io_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::MemberLength { dst, object } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ml_o_{tmp} = load %Value, %Value* %reg_{}\n",
                        object.0
                    ));
                    s.push_str(&format!(
                        "  %ml_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 18\n"
                    ));
                    s.push_str(&format!(
                        "  %ml_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %ml_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ml_res_{tmp} = call %Value %ml_fn_{tmp}(%Context* %ctx, %Value %ml_o_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %ml_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::Type { dst, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %t_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  %t_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 19\n"
                    ));
                    s.push_str(&format!(
                        "  %t_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %t_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %t_res_{tmp} = call %Value %t_fn_{tmp}(%Context* %ctx, %Value %t_v_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %t_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::Wait { dst, value } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %w_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        value.0
                    ));
                    s.push_str(&format!(
                        "  %w_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 20\n"
                    ));
                    s.push_str(&format!(
                        "  %w_fn_{tmp} = load %Value (%Context*, %Value)*, %Value (%Context*, %Value)** %w_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %w_res_{tmp} = call %Value %w_fn_{tmp}(%Context* %ctx, %Value %w_v_{tmp})\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %w_res_{tmp}, %Value* %reg_{}\n",
                        dst.0
                    ));
                }
                SsaInstruction::AddGlobal { slot, delta } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %ag_g_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 0\n"
                    ));
                    s.push_str(&format!(
                        "  %ag_g_{tmp} = load %Value*, %Value** %ag_g_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ag_sl_{tmp} = getelementptr %Value, %Value* %ag_g_{tmp}, i32 {}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  %ag_v_{tmp} = load %Value, %Value* %ag_sl_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %ag_num_{tmp} = extractvalue %Value %ag_v_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  %ag_next_{tmp} = add i64 %ag_num_{tmp}, {}\n",
                        delta
                    ));
                    s.push_str(&format!(
                        "  %ag_new_v_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %ag_next_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %ag_new_v_{tmp}, %Value* %ag_sl_{tmp}\n"
                    ));
                }
                SsaInstruction::AddLocal { slot, delta } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!(
                        "  %al_v_{tmp} = load %Value, %Value* %local_{}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  %al_num_{tmp} = extractvalue %Value %al_v_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  %al_next_{tmp} = add i64 %al_num_{tmp}, {}\n",
                        delta
                    ));
                    s.push_str(&format!(
                        "  %al_new_v_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %al_next_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  store %Value %al_new_v_{tmp}, %Value* %local_{}\n",
                        slot
                    ));
                }
                SsaInstruction::AppendStringGlobal { slot, suffix } => {
                    let tmp = self.fresh_id();
                    let str_idx = self.intern_string(suffix);
                    let len = suffix.len();
                    s.push_str(&format!(
                        "  %as_fn_ptr_{tmp} = getelementptr %Runtime, %Runtime* %rt, i32 0, i32 21\n"
                    ));
                    s.push_str(&format!(
                        "  %as_fn_{tmp} = load void (%Context*, i32, i8*, i32)*, void (%Context*, i32, i8*, i32)** %as_fn_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %as_s_raw_{tmp} = getelementptr inbounds [{} x i8], [{} x i8]* @kria_str_{}, i32 0, i32 0\n",
                        len, len, str_idx
                    ));
                    s.push_str(&format!(
                        "  call void %as_fn_{tmp}(%Context* %ctx, i32 {}, i8* %as_s_raw_{tmp}, i32 {})\n",
                        slot, len
                    ));
                }
                SsaInstruction::LoopIncLess {
                    slot,
                    limit,
                    step,
                    local,
                } => {
                    let tmp = self.fresh_id();
                    s.push_str(&format!("  br label %loop_inc_cond_{tmp}\n"));
                    s.push_str(&format!("loop_inc_cond_{tmp}:\n"));
                    if *local {
                        s.push_str(&format!(
                            "  %lic_cur_{tmp} = load %Value, %Value* %local_{}\n",
                            slot
                        ));
                    } else {
                        s.push_str(&format!(
                            "  %lic_g_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 0\n"
                        ));
                        s.push_str(&format!(
                            "  %lic_g_{tmp} = load %Value*, %Value** %lic_g_ptr_{tmp}\n"
                        ));
                        s.push_str(&format!(
                            "  %lic_sl_{tmp} = getelementptr %Value, %Value* %lic_g_{tmp}, i32 {}\n",
                            slot
                        ));
                        s.push_str(&format!(
                            "  %lic_cur_{tmp} = load %Value, %Value* %lic_sl_{tmp}\n"
                        ));
                    }
                    s.push_str(&format!(
                        "  %lic_num_{tmp} = extractvalue %Value %lic_cur_{tmp}, 1\n"
                    ));
                    s.push_str(&format!(
                        "  %lic_cmp_{tmp} = icmp slt i64 %lic_num_{tmp}, {}\n",
                        limit
                    ));
                    s.push_str(&format!(
                        "  br i1 %lic_cmp_{tmp}, label %loop_inc_body_{tmp}, label %loop_inc_exit_{tmp}\n"
                    ));

                    s.push_str(&format!("loop_inc_body_{tmp}:\n"));
                    s.push_str(&format!(
                        "  %lic_next_{tmp} = add i64 %lic_num_{tmp}, {}\n",
                        step
                    ));
                    s.push_str(&format!(
                        "  %lic_new_v_{tmp} = insertvalue %Value {{ i64 1, i64 0 }}, i64 %lic_next_{tmp}, 1\n"
                    ));
                    if *local {
                        s.push_str(&format!(
                            "  store %Value %lic_new_v_{tmp}, %Value* %local_{}\n",
                            slot
                        ));
                    } else {
                        s.push_str(&format!(
                            "  store %Value %lic_new_v_{tmp}, %Value* %lic_sl_{tmp}\n"
                        ));
                    }
                    s.push_str(&format!("  br label %loop_inc_cond_{tmp}\n"));

                    s.push_str(&format!("loop_inc_exit_{tmp}:\n"));
                }
                SsaInstruction::Return { .. } => {
                    // Handled by SsaTerminator::Return
                }
            }
        }

        s
    }

    fn emit_block_terminator(
        &mut self,
        term: &SsaTerminator,
        _next_block: Option<u32>,
        is_entry: bool,
    ) -> String {
        let mut s = String::new();
        match term {
            SsaTerminator::Fallthrough => {
                if is_entry {
                    s.push_str("  ret void\n");
                } else {
                    s.push_str("  ret %Value { i64 0, i64 0 }\n");
                }
            }
            SsaTerminator::Jump(target) => {
                s.push_str(&format!("  br label %block_{}\n", target.0));
            }
            SsaTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let tmp = self.fresh_id();
                s.push_str(&format!(
                    "  %br_c_{tmp} = load %Value, %Value* %reg_{}\n",
                    condition.0
                ));
                s.push_str(&format!(
                    "  %br_truthy_{tmp} = call i1 @kria_is_truthy(%Value %br_c_{tmp})\n"
                ));
                s.push_str(&format!(
                    "  br i1 %br_truthy_{tmp}, label %block_{}, label %block_{}\n",
                    then_block.0, else_block.0
                ));
            }
            SsaTerminator::CompareLessConst {
                slot,
                limit,
                local,
                then_block,
                else_block,
            } => {
                let tmp = self.fresh_id();
                if *local {
                    s.push_str(&format!(
                        "  %clc_v_{tmp} = load %Value, %Value* %local_{}\n",
                        slot
                    ));
                } else {
                    s.push_str(&format!(
                        "  %clc_g_ptr_{tmp} = getelementptr %Context, %Context* %ctx, i32 0, i32 0\n"
                    ));
                    s.push_str(&format!(
                        "  %clc_g_{tmp} = load %Value*, %Value** %clc_g_ptr_{tmp}\n"
                    ));
                    s.push_str(&format!(
                        "  %clc_sl_{tmp} = getelementptr %Value, %Value* %clc_g_{tmp}, i32 {}\n",
                        slot
                    ));
                    s.push_str(&format!(
                        "  %clc_v_{tmp} = load %Value, %Value* %clc_sl_{tmp}\n"
                    ));
                }
                s.push_str(&format!(
                    "  %clc_num_{tmp} = extractvalue %Value %clc_v_{tmp}, 1\n"
                ));
                s.push_str(&format!(
                    "  %clc_cmp_{tmp} = icmp slt i64 %clc_num_{tmp}, {}\n",
                    limit
                ));
                s.push_str(&format!(
                    "  br i1 %clc_cmp_{tmp}, label %block_{}, label %block_{}\n",
                    then_block.0, else_block.0
                ));
            }
            SsaTerminator::TailCall {
                function_index,
                args,
            } => {
                let tmp = self.fresh_id();
                let mut arg_strs = Vec::new();
                for (ai, arg) in args.iter().enumerate() {
                    s.push_str(&format!(
                        "  %tc_a_{}_{} = load %Value, %Value* %reg_{}\n",
                        tmp, ai, arg.0
                    ));
                    arg_strs.push(format!("%Value %tc_a_{}_{}", tmp, ai));
                }
                let args_joined = if arg_strs.is_empty() {
                    String::new()
                } else {
                    format!(", {}", arg_strs.join(", "))
                };
                s.push_str(&format!(
                    "  %tc_res_{tmp} = musttail call %Value @kria_func_direct_{}(%Context* %ctx, %Runtime* %rt, i64 0{})\n",
                    function_index, args_joined
                ));
                if is_entry {
                    s.push_str("  ret void\n");
                } else {
                    s.push_str(&format!("  ret %Value %tc_res_{tmp}\n"));
                }
            }
            SsaTerminator::Return(value) => {
                let tmp = self.fresh_id();
                if let Some(v) = value {
                    s.push_str(&format!(
                        "  %tret_v_{tmp} = load %Value, %Value* %reg_{}\n",
                        v.0
                    ));
                    if is_entry {
                        s.push_str("  ret void\n");
                    } else {
                        s.push_str(&format!("  ret %Value %tret_v_{tmp}\n"));
                    }
                } else {
                    if is_entry {
                        s.push_str("  ret void\n");
                    } else {
                        s.push_str("  ret %Value { i64 0, i64 0 }\n");
                    }
                }
            }
        }
        s
    }
}

#[cfg(target_os = "windows")]
fn lower_windows_runtime_abi(ir: &str) -> String {
    let mut output = String::with_capacity(ir.len() + 4096);
    let mut function_lines = Vec::new();
    let mut runtime_fields = HashMap::new();
    let mut callbacks = HashMap::new();
    let mut temporary_index = 0usize;
    let mut exported_sret: Option<String> = None;

    for line in ir.lines() {
        if line.starts_with("define ") {
            flush_windows_function(&mut output, &mut function_lines);
            exported_sret = None;
        }

        if function_lines.is_empty() && !line.starts_with("define ") {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        if line.starts_with("define ") {
            if let Some((definition, return_pointer)) = lower_windows_exported_function(line) {
                function_lines.push(definition);
                exported_sret = Some(return_pointer);
                continue;
            }
        }

        if line.trim() == "}" {
            function_lines.push(line.to_string());
            flush_windows_function(&mut output, &mut function_lines);
            runtime_fields.clear();
            callbacks.clear();
            exported_sret = None;
            continue;
        }

        if let Some(return_pointer) = &exported_sret {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("ret %Value ") {
                function_lines.push(format!(
                    "  store %Value {value}, %Value* {return_pointer}, align 8"
                ));
                function_lines.push("  ret void".to_string());
                continue;
            }
        }

        if let Some((pointer_name, field)) = runtime_field_pointer(line) {
            runtime_fields.insert(pointer_name, field);
            function_lines.push(line.to_string());
            continue;
        }

        if let Some((function_name, signature, pointer_name)) = loaded_runtime_callback(line) {
            if runtime_fields.contains_key(pointer_name.as_str()) {
                if let Some(abi_signature) = windows_callback_signature(&signature) {
                    let abi_name = format!("{}_kria_abi", function_name.trim_start_matches('%'));
                    let abi_function = format!("%{abi_name}");
                    function_lines.push(line.to_string());
                    function_lines.push(format!(
                        "  {abi_function} = bitcast {}* {function_name} to {}*",
                        signature, abi_signature.llvm_type
                    ));
                    callbacks.insert(function_name, (abi_function, abi_signature));
                    continue;
                }
            }
        }

        if let Some((result_name, return_type, function_name, arguments)) =
            runtime_callback_call(line)
        {
            if let Some((abi_function, signature)) = callbacks.get(function_name.as_str()) {
                let mut call_arguments = Vec::new();
                let mut setup = Vec::new();
                let mut argument_index = 0;

                let output_name = if signature.returns_value {
                    let name = format!("%kria_abi_{temporary_index}_out");
                    temporary_index += 1;
                    function_lines.push(format!("  {name} = alloca %Value, align 8"));
                    call_arguments.push(format!("%Value* sret([16 x i8]) align 8 {name}"));
                    Some(name)
                } else {
                    None
                };

                for (argument_type, argument_value) in arguments {
                    if argument_type == "%Value" {
                        let name = format!("%kria_abi_{temporary_index}_arg_{argument_index}");
                        temporary_index += 1;
                        setup.push(format!("  {name} = alloca %Value, align 8"));
                        setup.push(format!(
                            "  store %Value {argument_value}, %Value* {name}, align 8"
                        ));
                        call_arguments.push(format!("%Value* align 8 {name}"));
                        argument_index += 1;
                    } else {
                        call_arguments.push(format!("{argument_type} {argument_value}"));
                    }
                }

                function_lines.extend(setup);
                let call_return_type = if signature.returns_value {
                    "void"
                } else {
                    &return_type
                };
                let call_result = if signature.returns_value {
                    String::new()
                } else {
                    result_name
                        .as_deref()
                        .map(|name| format!("{name} = "))
                        .unwrap_or_default()
                };
                function_lines.push(format!(
                    "  {call_result}call {call_return_type} {abi_function}({})",
                    call_arguments.join(", ")
                ));
                if let (Some(result_name), Some(output_name)) = (result_name, output_name) {
                    function_lines.push(format!(
                        "  {result_name} = load %Value, %Value* {output_name}, align 8"
                    ));
                }
                continue;
            }
        }

        function_lines.push(line.to_string());
    }

    flush_windows_function(&mut output, &mut function_lines);
    output
}

#[cfg(target_os = "windows")]
struct WindowsCallbackSignature {
    llvm_type: String,
    returns_value: bool,
}

#[cfg(target_os = "windows")]
fn windows_callback_signature(signature: &str) -> Option<WindowsCallbackSignature> {
    let open = signature.find('(')?;
    let close = signature.rfind(')')?;
    let return_type = signature[..open].trim();
    let parameters = split_llvm_arguments(&signature[open + 1..close]);
    let returns_value = return_type == "%Value";
    let mut lowered_parameters = Vec::with_capacity(parameters.len() + usize::from(returns_value));

    if returns_value {
        lowered_parameters.push("%Value*".to_string());
    }
    for parameter in parameters {
        let parameter = parameter.trim();
        lowered_parameters.push(if parameter == "%Value" {
            "%Value*".to_string()
        } else {
            parameter.to_string()
        });
    }

    let lowered_return_type = if returns_value { "void" } else { return_type };
    Some(WindowsCallbackSignature {
        llvm_type: format!(
            "{} ({})",
            lowered_return_type,
            lowered_parameters.join(", ")
        ),
        returns_value,
    })
}

#[cfg(target_os = "windows")]
fn lower_windows_exported_function(line: &str) -> Option<(String, String)> {
    let declaration = line.strip_prefix("define %Value @kria_func_")?;
    let name_end = declaration.find('(')?;
    let function_suffix = &declaration[..name_end];
    if function_suffix.starts_with("direct_") {
        return None;
    }

    let return_pointer = "%kria_result".to_string();
    let parameters = &declaration[name_end + 1..declaration.rfind(')')?];
    Some((
        format!(
            "define void @kria_func_{function_suffix}(%Value* sret([16 x i8]) align 8 {return_pointer}, {parameters}) {{"
        ),
        return_pointer,
    ))
}

#[cfg(target_os = "windows")]
fn runtime_field_pointer(line: &str) -> Option<(String, usize)> {
    if !line.contains("getelementptr %Runtime") {
        return None;
    }
    let (name, _) = line.trim().split_once(" = ")?;
    let field = line.rsplit_once(", i32 ")?.1.trim().parse().ok()?;
    Some((name.to_string(), field))
}

#[cfg(target_os = "windows")]
fn loaded_runtime_callback(line: &str) -> Option<(String, String, String)> {
    if !line.contains(" = load ") {
        return None;
    }
    let (name, expression) = line.trim().split_once(" = load ")?;
    let parts = split_llvm_arguments(expression);
    let function_type = parts.first()?.trim().strip_suffix('*')?.trim();
    let pointer_name = expression.rsplit_once(' ')?.1.trim();
    if !function_type.contains('(') || !pointer_name.starts_with('%') {
        return None;
    }
    Some((
        name.to_string(),
        function_type.to_string(),
        pointer_name.to_string(),
    ))
}

#[cfg(target_os = "windows")]
fn runtime_callback_call(
    line: &str,
) -> Option<(Option<String>, String, String, Vec<(String, String)>)> {
    let trimmed = line.trim();
    let call_index = trimmed.find("call ")?;
    let prefix = trimmed[..call_index].trim();
    let result_name = prefix
        .strip_suffix('=')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let call = &trimmed[call_index + "call ".len()..];
    let open = call.find('(')?;
    let close = call.rfind(')')?;
    let mut header = call[..open].split_whitespace();
    let return_type = header.next()?.to_string();
    let function_name = header.next()?.to_string();
    if !function_name.starts_with('%') {
        return None;
    }
    let arguments = split_llvm_arguments(&call[open + 1..close])
        .into_iter()
        .map(|argument| {
            let argument = argument.trim();
            let split = argument.find(' ')?;
            Some((
                argument[..split].to_string(),
                argument[split + 1..].trim().to_string(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((result_name, return_type, function_name, arguments))
}

#[cfg(target_os = "windows")]
fn split_llvm_arguments(arguments: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in arguments.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&arguments[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < arguments.len() {
        parts.push(&arguments[start..]);
    }
    parts
}

#[cfg(target_os = "windows")]
fn flush_windows_function(output: &mut String, function_lines: &mut Vec<String>) {
    if function_lines.is_empty() {
        return;
    }
    let alloca_index = function_lines
        .iter()
        .position(|line| line.trim() == "entry:")
        .map(|index| index + 1);
    if let Some(index) = alloca_index {
        let mut allocation_lines = Vec::new();
        for line in function_lines.iter() {
            let trimmed = line.trim();
            if trimmed.starts_with("%kria_abi_") && trimmed.contains(" = alloca %Value") {
                allocation_lines.push(line.clone());
            }
        }
        function_lines.retain(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("%kria_abi_") && trimmed.contains(" = alloca %Value"))
        });
        for (offset, line) in allocation_lines.into_iter().enumerate() {
            function_lines.insert(index + offset, line);
        }
    }
    for line in function_lines.drain(..) {
        output.push_str(&line);
        output.push('\n');
    }
}

fn escape_llvm_string(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if b >= 32 && b <= 126 && b != b'\\' && b != b'"' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\{:02X}", b));
        }
    }
    out
}
