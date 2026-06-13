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
            term: None,
            sealed: false,
        }
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
        )
    }
}
