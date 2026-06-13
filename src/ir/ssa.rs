//! SSA block sealing helpers (Braun et al. style).

use super::builder::IrBuilder;
use super::types::IrModule;
use super::value::BlockId;

pub fn seal_block(builder: &mut IrBuilder, module: &mut IrModule, block: BlockId) {
    builder.seal_block(module, block);
}

pub fn merge_blocks(
    builder: &mut IrBuilder,
    module: &mut IrModule,
    then_end: BlockId,
    else_end: BlockId,
) -> BlockId {
    let merge = builder.new_block(module);
    builder.add_pred(module, merge, then_end);
    builder.add_pred(module, merge, else_end);
    builder.seal_block(module, merge);
    merge
}
