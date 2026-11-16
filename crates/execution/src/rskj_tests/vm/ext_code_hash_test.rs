//! rskj's `co.rsk.vm.ExtCodeHashTest`, its test of RSKIP140 (papyrus200):
//! EXTCODEHASH is an opcode from it, and an invalid one before it.

use alloy_primitives::{keccak256, B256};

use super::harness::{compile, return_top, run_code, GAS};

/// papyrus200 on mainnet: RSKIP140.
const PAPYRUS: u64 = 2_392_700;

/// Before RSKIP140 EXTCODEHASH is an invalid opcode: the contract that runs
/// it fails with all its gas. From RSKIP140 it pushes the hash of the code
/// at the address, here the contract's own.
#[test]
fn extcodehash_is_an_invalid_opcode_before_rskip140() {
    let mut code = compile("ADDRESS EXTCODEHASH");
    code.extend(return_top(1));
    let before = run_code(PAPYRUS - 1, &code);
    let test = "testDoEXTCODEHASHWithHardForkDeactivated";
    assert!(!before.success, "{test}");
    assert_eq!(before.gas_used, GAS, "{test}");
    let from = run_code(PAPYRUS, &code);
    assert!(from.success, "{test}");
    assert_eq!(B256::from_slice(&from.output), keccak256(&code), "{test}");
}
