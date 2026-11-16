//! rskj's `org.ethereum.core.NestedCallsStackDepthTest`, its test with
//! RSKIP203 (arrowhead600) off, which it turns off by leaving arrowhead600
//! unscheduled on its regtest, where every later upgrade is at block 0: the
//! Environment contract is not there, and a contract's call of its
//! getCallStackDepth gets nothing back (rskj's Solidity caller then
//! reverts). Mainnet's window is held by `precompiles.rs`'s
//! `test_environment_active_after_arrowhead600`.

use alloy_primitives::{hex, keccak256, U256};

use super::harness::{compile, push_address, return_top, run_code_on};
use crate::hardfork::{RskHardforkConfig, RskNetworkUpgrade};
use crate::precompiles::ENVIRONMENT_ADDR;

/// With arrowhead600 unscheduled a contract's call of the Environment
/// contract's getCallStackDepth succeeds with no data back.
#[test]
#[ignore = "rskj leaves RSKIP203 off where arrowhead600 is unscheduled though a later upgrade \
            is scheduled (ActivationConfig.isActive reads arrowhead600's height), so no \
            Environment contract is at its address (PrecompiledContracts.getContractForAddress); \
            rustock's precompile set (precompiles.rs rsk_precompiles) holds it from any upgrade \
            past arrowhead600, and it answers"]
fn no_environment_contract_answers_where_arrowhead600_is_unscheduled() {
    let selector = hex::encode(&keccak256("getCallStackDepth()")[..4]);
    let mut code = compile(&format!(
        "PUSH4 0x{selector} PUSH1 0x00 MSTORE PUSH1 0x00 PUSH1 0x00 PUSH1 0x04 PUSH1 0x1c \
         PUSH1 0x00 {} PUSH3 0x0186a0 CALL RETURNDATASIZE",
        push_address(ENVIRONMENT_ADDR)
    ));
    code.extend(return_top(2));
    let regtest = RskHardforkConfig::rskj_regtest();
    let config = regtest.with_hardfork(RskNetworkUpgrade::Arrowhead600, None);
    let ran = run_code_on(config, 1, &code);
    let test = "testContractConsensusInactive";
    assert!(ran.success, "{test}");
    let [size, called] = [&ran.output[..32], &ran.output[32..]].map(U256::from_be_slice);
    assert_eq!(called, U256::from(1), "{test}");
    assert_eq!(size, U256::ZERO, "{test}");
}
