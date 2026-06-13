use std::collections::HashMap;

use crate::bytecode::{CAPTURE_LOCAL, CAPTURE_UPVALUE};
use crate::vm::Value;

use super::inst::{IrBlock, IrInst, IrTerminator};
use super::value::{BlockId, FunctionId};

#[derive(Debug, Clone)]
pub struct CaptureDesc {
    pub kind: u8,
    pub index: u32,
}

impl CaptureDesc {
    pub fn local(index: u32) -> Self {
        CaptureDesc {
            kind: CAPTURE_LOCAL,
            index,
        }
    }

    pub fn upvalue(index: u32) -> Self {
        CaptureDesc {
            kind: CAPTURE_UPVALUE,
            index,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrGlobal {
    pub name: String,
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: String,
    pub local_index: u32,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: Option<String>,
    pub params: Vec<IrParam>,
    pub blocks: Vec<IrBlock>,
    pub entry: BlockId,
    pub captures: Vec<CaptureDesc>,
    pub num_locals: u32,
}

#[derive(Debug, Clone)]
pub struct IrModule {
    pub globals: Vec<IrGlobal>,
    pub global_map: HashMap<String, u32>,
    pub constants: Vec<Value>,
    pub functions: Vec<IrFunction>,
    /// Module init / top-level code.
    pub entry: FunctionId,
    /// High-level loop hints for fused opcode emission.
    pub fused_loops: Vec<FusedLoopHint>,
}

#[derive(Debug, Clone)]
pub enum FusedLoopHint {
    CounterIncLess {
        global_idx: u32,
        limit: i64,
    },
    CounterStepLess {
        global_idx: u32,
        limit: i64,
        step: i64,
    },
    LessConstJumpWhile {
        global_idx: u32,
        limit: i64,
        body: Vec<IrInst>,
        hoisted_globals: Vec<(u32, u32)>,
    },
}

impl IrModule {
    pub fn new() -> Self {
        let entry_fn = IrFunction {
            name: Some("<module>".to_string()),
            params: Vec::new(),
            blocks: vec![IrBlock::new(BlockId(0))],
            entry: BlockId(0),
            captures: Vec::new(),
            num_locals: 0,
        };
        IrModule {
            globals: Vec::new(),
            global_map: HashMap::new(),
            constants: Vec::new(),
            functions: vec![entry_fn],
            entry: FunctionId(0),
            fused_loops: Vec::new(),
        }
    }

    pub fn entry_function(&self) -> &IrFunction {
        &self.functions[self.entry.0 as usize]
    }

    pub fn entry_function_mut(&mut self) -> &mut IrFunction {
        let id = self.entry.0 as usize;
        &mut self.functions[id]
    }

    pub fn resolve_global(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.global_map.get(name) {
            return idx;
        }
        let idx = self.globals.len() as u32;
        self.globals.push(IrGlobal {
            name: name.to_string(),
            index: idx,
        });
        self.global_map.insert(name.to_string(), idx);
        idx
    }

    pub fn add_constant(&mut self, value: Value) -> u32 {
        for (i, c) in self.constants.iter().enumerate() {
            if *c == value {
                return i as u32;
            }
        }
        let idx = self.constants.len() as u32;
        self.constants.push(value);
        idx
    }

    pub fn num_globals(&self) -> usize {
        self.globals.len()
    }
}

impl Default for IrModule {
    fn default() -> Self {
        Self::new()
    }
}

impl IrFunction {
    pub fn new(name: Option<String>, params: Vec<IrParam>) -> Self {
        IrFunction {
            name,
            params,
            blocks: vec![IrBlock::new(BlockId(0))],
            entry: BlockId(0),
            captures: Vec::new(),
            num_locals: 0,
        }
    }

    pub fn block(&self, id: BlockId) -> &IrBlock {
        &self.blocks[id.0 as usize]
    }

    pub fn block_mut(&mut self, id: BlockId) -> &mut IrBlock {
        &mut self.blocks[id.0 as usize]
    }

    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(IrBlock::new(id));
        id
    }
}
