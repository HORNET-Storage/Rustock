//! rskj's `co.rsk.vm.opcode.MCopyDslTest`, its test of MCOPY with RSKIP445
//! (lovell700) off, which it turns off by leaving lovell700 unscheduled on
//! its regtest, where every later upgrade is at block 0. Mainnet's window is
//! held by `vm_execution_test`'s `mcopy_is_an_invalid_opcode_before_rskip445`.

use super::harness::{compile, run_code_on, GAS};
use crate::hardfork::{RskHardforkConfig, RskNetworkUpgrade};

/// With lovell700 unscheduled MCOPY is an invalid opcode: the transaction
/// that runs it fails with all its gas.
#[test]
#[ignore = "rskj leaves RSKIP445 off where lovell700 is unscheduled though a later upgrade is \
            scheduled (ActivationConfig.isActive reads lovell700's height), so MCOPY throws as \
            an invalid opcode (VM.doMCOPY); rustock's has_rskip445 (hardfork.rs) turns it on \
            from any upgrade past lovell700, and MCOPY runs"]
fn mcopy_is_an_invalid_opcode_where_lovell700_is_unscheduled() {
    let regtest = RskHardforkConfig::rskj_regtest();
    let config = regtest.with_hardfork(RskNetworkUpgrade::Lovell700, None);
    let code = compile("PUSH1 0x20 PUSH1 0x00 PUSH1 0x20 MCOPY STOP");
    let ran = run_code_on(config, 1, &code);
    let test = "testMCOPY_whenNotActivated_behavesAsExpected";
    assert!(!ran.success, "{test}");
    assert_eq!(ran.gas_used, GAS, "{test}");
}
