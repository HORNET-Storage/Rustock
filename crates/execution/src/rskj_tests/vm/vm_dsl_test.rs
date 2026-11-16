//! rskj's `co.rsk.vm.VmDslTest`, its tests of RSKIP438 (lovell700): from it
//! CREATE and CREATE2 charge 2 gas for each word of the init code they run
//! (`VM.doCREATE`, `VM.doCREATE2`), and init code past 49,152 bytes fails
//! the contract that runs them (`Program.validateInitcodeSize`).

use alloy_primitives::U256;

use super::harness::{compile, return_top, run_code_on, GAS};
use crate::hardfork::{RskHardforkConfig, RskNetworkUpgrade};

/// lovell700 on mainnet: RSKIP438.
const LOVELL: u64 = 7_338_024;
/// The most init code RSKIP438 lets a creation run: twice the most code a
/// contract may hold.
const MAX_INITCODE: usize = 49_152;

/// What a contract gets from creating, by `opcode`, a contract of `size`
/// zero bytes of init code (a STOP, which deploys no code), in block
/// `number` of `config`: whether the transaction succeeded, the address
/// the contract returns, and the gas it used.
fn create_on(
    config: RskHardforkConfig,
    number: u64,
    opcode: &str,
    size: usize,
) -> (bool, U256, u64) {
    let salt = if opcode == "CREATE2" {
        "PUSH1 0x01"
    } else {
        ""
    };
    let mut code = compile(&format!(
        "{salt} PUSH3 0x{size:06x} PUSH1 0x00 PUSH1 0x00 {opcode}"
    ));
    code.extend(return_top(1));
    let ran = run_code_on(config, number, &code);
    (ran.success, U256::from_be_slice(&ran.output), ran.gas_used)
}

/// The same on mainnet.
fn create_at(number: u64, opcode: &str, size: usize) -> (bool, U256, u64) {
    create_on(RskHardforkConfig::mainnet(), number, opcode, size)
}

/// Before RSKIP438 CREATE and CREATE2 run init code of any size, and charge
/// for its memory and, CREATE2, its hashing, but nothing for the code: one
/// byte past 49,152 creates the contract too.
#[test]
fn before_rskip438_create_and_create2_run_init_code_of_any_size_for_no_gas() {
    let cases = [
        ("CREATE", MAX_INITCODE, 62_237),
        ("CREATE", MAX_INITCODE + 1, 62_246),
        ("CREATE2", MAX_INITCODE, 71_456),
        ("CREATE2", MAX_INITCODE + 1, 71_471),
    ];
    for (opcode, size, gas) in cases {
        let (success, created, used) = create_at(LOVELL - 1, opcode, size);
        assert!(success && created != U256::ZERO, "{opcode} of {size} bytes");
        assert_eq!(used, gas, "{opcode} of {size} bytes");
    }
}

/// From RSKIP438 CREATE and CREATE2 charge 2 gas for each word of init
/// code: 3,072 more for 49,152 bytes.
#[test]
fn from_rskip438_create_and_create2_charge_2_gas_a_word_of_init_code() {
    let cases = [
        (
            "testInitCodeSizeCREATEOpcodeValidationSuccessWithInitcodeCost",
            "CREATE",
            65_309,
        ),
        (
            "testInitCodeSizeCREATE2OpcodeValidationSuccessWithInitCodeCost",
            "CREATE2",
            74_528,
        ),
    ];
    for (test, opcode, gas) in cases {
        let (success, created, used) = create_at(LOVELL, opcode, MAX_INITCODE);
        assert!(success && created != U256::ZERO, "{test}");
        assert_eq!(used, gas, "{test}");
    }
}

/// From RSKIP438 init code past 49,152 bytes fails the contract that runs
/// CREATE or CREATE2, with all its gas.
#[test]
fn from_rskip438_init_code_past_49152_bytes_fails_the_contract_that_creates() {
    let cases = [
        (
            "testInitCodeSizeCREATEOpcodeValidationFailWithRSKIP438Activated",
            "CREATE",
        ),
        (
            "testInitCodeSizeCREATE2OpcodeValidationFailIfRSKIP438Activated",
            "CREATE2",
        ),
    ];
    for (test, opcode) in cases {
        let (success, _, used) = create_at(LOVELL, opcode, MAX_INITCODE + 1);
        assert!(!success, "{test}");
        assert_eq!(used, GAS, "{test}");
    }
}

/// rskj's tests turn RSKIP438 off by leaving lovell700 unscheduled on their
/// regtest, where every later upgrade is at block 0: CREATE and CREATE2 then
/// charge nothing for init code, and take more than 49,152 bytes of it.
#[test]
#[ignore = "rskj leaves RSKIP438 off where lovell700 is unscheduled though a later upgrade is \
            scheduled (ActivationConfig.isActive reads lovell700's height), so CREATE and \
            CREATE2 run init code of any size for no gas (Program.validateInitcodeSize, \
            InitcodeCostCalculator.calculateCost); rustock runs revm's SHANGHAI spec, with \
            EIP-3860's limit and price, from any upgrade past lovell700 \
            (hardfork.rs upgrade_to_spec_id)"]
fn create_and_create2_run_init_code_of_any_size_for_no_gas_where_lovell700_is_unscheduled() {
    let cases = [
        (
            "testInitCodeSizeCREATEOpcodeValidationSuccessWithoutInitcodeCost",
            "CREATE",
            62_237,
        ),
        (
            "testInitCodeSizeCREATE2OpcodeValidationSuccessWithoutInitcodeCost",
            "CREATE2",
            71_456,
        ),
    ];
    let unscheduled = || {
        let regtest = RskHardforkConfig::rskj_regtest();
        regtest.with_hardfork(RskNetworkUpgrade::Lovell700, None)
    };
    for (test, opcode, gas) in cases {
        let (success, _, used) = create_on(unscheduled(), 1, opcode, MAX_INITCODE);
        assert!(success, "{test}");
        assert_eq!(used, gas, "{test}");
    }
    let cases = [
        (
            "testInitCodeSizeCREATEOpcodeValidationDoesntFailWithRSKIP438Deactivated",
            "CREATE",
        ),
        (
            "testInitCodeSizeCREATE2OpcodeValidationDoesntFailIfRSKIP438Deactivated",
            "CREATE2",
        ),
    ];
    for (test, opcode) in cases {
        let (success, created, _) = create_on(unscheduled(), 1, opcode, MAX_INITCODE + 1);
        assert!(success && created != U256::ZERO, "{test}");
    }
}
