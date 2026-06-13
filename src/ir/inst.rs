use crate::ast::BinaryOperator;

use super::value::{BlockId, FunctionId, ValueId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl From<BinaryOperator> for IrBinOp {
    fn from(op: BinaryOperator) -> Self {
        match op {
            BinaryOperator::Add => IrBinOp::Add,
            BinaryOperator::Subtract => IrBinOp::Sub,
            BinaryOperator::Multiply => IrBinOp::Mul,
            BinaryOperator::Divide => IrBinOp::Div,
            BinaryOperator::Equals => IrBinOp::Eq,
            BinaryOperator::NotEquals => IrBinOp::Ne,
            BinaryOperator::GreaterThan => IrBinOp::Gt,
            BinaryOperator::LessThan => IrBinOp::Lt,
            BinaryOperator::GreaterOrEqual => IrBinOp::Ge,
            BinaryOperator::LessOrEqual => IrBinOp::Le,
            BinaryOperator::And | BinaryOperator::Or => unreachable!("handled via CFG"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrPhi {
    pub result: ValueId,
    pub var_name: String,
    pub incoming: Vec<(ValueId, BlockId)>,
}

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub id: BlockId,
    pub preds: Vec<BlockId>,
    pub phis: Vec<IrPhi>,
    pub insts: Vec<IrInst>,
    /// Source bytecode IP per instruction (lift metadata for jump fixup).
    pub inst_ips: Vec<u32>,
    pub term: Option<IrTerminator>,
    pub sealed: bool,
}

impl IrBlock {
    pub fn new(id: BlockId) -> Self {
        IrBlock {
            id,
            preds: Vec::new(),
            phis: Vec::new(),
            insts: Vec::new(),
            inst_ips: Vec::new(),
            term: None,
            sealed: false,
        }
    }

    pub fn push_inst(&mut self, ip: u32, inst: IrInst) {
        self.inst_ips.push(ip);
        self.insts.push(inst);
    }
}

#[derive(Debug, Clone)]
pub enum IrInst {
    Const {
        result: ValueId,
        idx: u32,
    },
    Null {
        result: ValueId,
    },
    LoadGlobal {
        result: ValueId,
        slot: u32,
    },
    StoreGlobal {
        slot: u32,
        value: ValueId,
    },
    IncGlobal {
        slot: u32,
    },
    AddGlobal {
        slot: u32,
        delta: i64,
    },
    LoadLocal {
        result: ValueId,
        slot: u32,
    },
    StoreLocal {
        slot: u32,
        value: ValueId,
    },
    LoadUpvalue {
        result: ValueId,
        slot: u32,
    },
    StoreUpvalue {
        slot: u32,
        value: ValueId,
    },
    BinOp {
        result: ValueId,
        op: IrBinOp,
        left: ValueId,
        right: ValueId,
    },
    Not {
        result: ValueId,
        value: ValueId,
    },
    Print {
        value: ValueId,
    },
    Pop {
        value: ValueId,
    },
    Input {
        result: ValueId,
        prompt: ValueId,
        type_mask: u8,
    },
    Call {
        result: ValueId,
        callee: ValueId,
        args: Vec<ValueId>,
    },
    MakeClosure {
        result: ValueId,
        func: FunctionId,
    },
    BuildArray {
        result: ValueId,
        elements: Vec<ValueId>,
        mutable: bool,
    },
    BuildObject {
        result: ValueId,
        field_values: Vec<ValueId>,
        field_keys: Vec<u32>,
    },
    IndexGet {
        result: ValueId,
        object: ValueId,
        index: ValueId,
    },
    IndexSet {
        object: ValueId,
        index: ValueId,
        value: ValueId,
    },
    ArrayLen {
        result: ValueId,
        array: ValueId,
    },
    ArrayPush {
        array: ValueId,
        value: ValueId,
    },
    ArrayPop {
        result: ValueId,
        array: ValueId,
    },
    ObjectGetConst {
        result: ValueId,
        object: ValueId,
        key_idx: u32,
    },
    ObjectGet {
        result: ValueId,
        object: ValueId,
        key: ValueId,
    },
    ObjectSet {
        object: ValueId,
        key: ValueId,
        value: ValueId,
    },
    ObjectDelete {
        object: ValueId,
        key: ValueId,
    },
    ObjectKeys {
        result: ValueId,
        object: ValueId,
    },
    ObjectGetOrCreateConst {
        result: ValueId,
        object: ValueId,
        key_idx: u32,
    },
    ObjectGetOrCreate {
        result: ValueId,
        object: ValueId,
        key: ValueId,
    },
    IsArray {
        result: ValueId,
        value: ValueId,
    },
    IsObject {
        result: ValueId,
        value: ValueId,
    },
    MemberLength {
        result: ValueId,
        object: ValueId,
    },
    Type {
        result: ValueId,
        value: ValueId,
    },
    Wait {
        value: ValueId,
    },
    /// Fused counter loop: while (i < limit) { i += 1 }
    LoopIncLess {
        global_idx: u32,
        limit: i64,
    },
    /// Fused counter loop with step
    LoopStepLess {
        global_idx: u32,
        limit: i64,
        step: i64,
    },
    /// Side-effect only path assignment prelude.
    StorePathCur {
        slot: u32,
        value: ValueId,
    },
    LoadPathCur {
        result: ValueId,
        slot: u32,
    },
    /// Unconditional branch (bytecode IP label).
    Jump {
        target_ip: u32,
    },
    /// Pop boolean; branch when false (bytecode IP label).
    JumpIfFalse {
        target_ip: u32,
    },
    Return {
        value: Option<ValueId>,
    },
    /// Fused while (i < limit) test + back-edge.
    LessConstJumpIfFalse {
        global_idx: u32,
        limit: i64,
        target_ip: u32,
    },
    /// for item in array: load next item or exit.
    ForInArrayHeader {
        arr_slot: u32,
        i_slot: u32,
        exit_ip: u32,
    },
    /// for item in array: increment index and loop.
    ForInArrayNext {
        i_slot: u32,
        loop_start_ip: u32,
    },
}

#[derive(Debug, Clone)]
pub enum IrTerminator {
    Jump {
        target: BlockId,
    },
    Branch {
        cond: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return {
        value: Option<ValueId>,
    },
    /// Fused for-in array loop body entry (emit maps to OP_FOR_IN_ARRAY_*).
    ForInArray {
        arr_slot: u32,
        i_slot: u32,
        item_local: u32,
        body: BlockId,
        continue_block: BlockId,
        exit_block: BlockId,
    },
}

impl IrInst {
    pub fn result(&self) -> Option<ValueId> {
        match self {
            IrInst::Const { result, .. }
            | IrInst::Null { result }
            | IrInst::LoadGlobal { result, .. }
            | IrInst::LoadLocal { result, .. }
            | IrInst::LoadUpvalue { result, .. }
            | IrInst::BinOp { result, .. }
            | IrInst::Not { result, .. }
            | IrInst::Input { result, .. }
            | IrInst::Call { result, .. }
            | IrInst::MakeClosure { result, .. }
            | IrInst::BuildArray { result, .. }
            | IrInst::BuildObject { result, .. }
            | IrInst::IndexGet { result, .. }
            | IrInst::ArrayLen { result, .. }
            | IrInst::ArrayPop { result, .. }
            | IrInst::ObjectGetConst { result, .. }
            | IrInst::ObjectGet { result, .. }
            | IrInst::ObjectKeys { result, .. }
            | IrInst::ObjectGetOrCreateConst { result, .. }
            | IrInst::ObjectGetOrCreate { result, .. }
            | IrInst::IsArray { result, .. }
            | IrInst::IsObject { result, .. }
            | IrInst::MemberLength { result, .. }
            | IrInst::Type { result, .. }
            | IrInst::LoadPathCur { result, .. } => Some(*result),
            IrInst::Return { value } => *value,
            _ => None,
        }
    }

    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            IrInst::StoreGlobal { .. }
                | IrInst::IncGlobal { .. }
                | IrInst::AddGlobal { .. }
                | IrInst::StoreLocal { .. }
                | IrInst::StoreUpvalue { .. }
                | IrInst::Print { .. }
                | IrInst::Pop { .. }
                | IrInst::IndexSet { .. }
                | IrInst::ArrayPush { .. }
                | IrInst::ObjectSet { .. }
                | IrInst::ObjectDelete { .. }
                | IrInst::Wait { .. }
                | IrInst::StorePathCur { .. }
                | IrInst::Return { .. }
                | IrInst::Jump { .. }
                | IrInst::JumpIfFalse { .. }
                | IrInst::LessConstJumpIfFalse { .. }
                | IrInst::ForInArrayHeader { .. }
                | IrInst::ForInArrayNext { .. }
        )
    }

    /// Original bytecode size for roundtrip jump fixup.
    pub fn lifted_byte_size(&self) -> usize {
        use crate::bytecode::*;
        match self {
            IrInst::Const { .. } => 1 + 4,
            IrInst::Null { .. } => 1,
            IrInst::LoadGlobal { .. }
            | IrInst::StoreGlobal { .. }
            | IrInst::LoadLocal { .. }
            | IrInst::StoreLocal { .. }
            | IrInst::LoadUpvalue { .. }
            | IrInst::StoreUpvalue { .. }
            | IrInst::IncGlobal { .. } => 1 + 4,
            IrInst::AddGlobal { .. } => 1 + 4 + 8,
            IrInst::BinOp { .. } => 1,
            IrInst::Not { .. } | IrInst::Print { .. } | IrInst::Pop { .. } => 1,
            IrInst::Input { .. } => 1 + 1,
            IrInst::Call { args, .. } => 1 + 4,
            IrInst::MakeClosure { func, .. } => {
                let _ = func;
                1 + 4 + 4 + 4 // + captures patched via lifted_byte_size_with
            }
            IrInst::BuildArray { elements, .. } => 1 + 4 + 1,
            IrInst::BuildObject { field_keys, .. } => 1 + 4 + field_keys.len() * 4,
            IrInst::IndexGet { .. }
            | IrInst::IndexSet { .. }
            | IrInst::ArrayLen { .. }
            | IrInst::ArrayPop { .. }
            | IrInst::ObjectGet { .. }
            | IrInst::ObjectSet { .. }
            | IrInst::ObjectDelete { .. }
            | IrInst::ObjectKeys { .. }
            | IrInst::ObjectGetOrCreate { .. }
            | IrInst::IsArray { .. }
            | IrInst::IsObject { .. }
            | IrInst::MemberLength { .. }
            | IrInst::Type { .. } => 1,
            IrInst::ObjectGetConst { .. }
            | IrInst::ObjectGetOrCreateConst { .. } => 1 + 4,
            IrInst::ArrayPush { .. } => 1 + 1,
            IrInst::Wait { .. } => 1 + 1,
            IrInst::LoopIncLess { .. } => 1 + 4 + 8,
            IrInst::LoopStepLess { .. } => 1 + 4 + 8 + 8,
            IrInst::Jump { .. } | IrInst::JumpIfFalse { .. } => 1 + 4,
            IrInst::Return { .. } => 1,
            IrInst::LessConstJumpIfFalse { .. } => 1 + 4 + 8 + 4,
            IrInst::ForInArrayHeader { .. } => 1 + 4 + 4 + 4,
            IrInst::ForInArrayNext { .. } => 1 + 4 + 4,
            IrInst::StorePathCur { .. } | IrInst::LoadPathCur { .. } => 1 + 4,
        }
    }

    pub fn lifted_byte_size_with(&self, module: &super::types::IrModule) -> usize {
        match self {
            IrInst::MakeClosure { func, .. } => {
                let f = &module.functions[func.0 as usize];
                1 + 4 + 4 + 4 + f.captures.len() * 5
            }
            other => other.lifted_byte_size(),
        }
    }
}
