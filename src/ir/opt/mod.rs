pub mod constant_fold;
pub mod copy_prop;
pub mod dce;
pub mod member_hoist;

use super::types::IrModule;

pub fn run_ir_passes(module: &mut IrModule) {
    constant_fold::run(module);
    member_hoist::run(module);
    copy_prop::run(module);
    dce::run(module);
}
