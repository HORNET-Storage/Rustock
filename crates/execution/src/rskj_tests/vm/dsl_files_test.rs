//! rskj's `co.rsk.test.DslFilesTest`, its tests that name a rule: the gas of
//! a creation before RSKIP438 (lovell700) and from it, and a deployment onto
//! an address that holds a balance before RSKIP174 (iris300).

use alloy_primitives::{hex, Address, U256};

use super::harness::{call, create, Chain, GAS, ONE_RBTC, SENDER};
use crate::hardfork::{RskHardforkConfig, RskNetworkUpgrade};

/// lovell700 on mainnet: RSKIP438.
const LOVELL: u64 = 7_338_024;
/// The sender that funds the address of the sender's first creation.
const FUNDER: Address = Address::repeat_byte(0xa2);

/// The init code of rskj's `create01.txt`, solc's empty contract: 80 bytes,
/// three words.
fn empty_contract() -> Vec<u8> {
    let code = [
        "60606040523415600e57600080fd5b603580601b6000396000f3006060604052600080fd00",
        "a165627a7a72305820b25edb28bec763685838b8044760e105b5385638276b4768c8045237b8fc",
        "6bf10029",
    ];
    hex::decode(code.concat()).expect("hex")
}

/// The gas the creation of rskj's empty contract uses, in block `number` of
/// `config`.
fn creation_gas(config: RskHardforkConfig, number: u64) -> u64 {
    let mut chain = Chain::new(config, number);
    chain.put(SENDER, 0, ONE_RBTC, &[]);
    let ran = chain.run(SENDER, create(0, 500_000, &empty_contract()));
    assert!(ran.success, "the creation runs");
    ran.gas_used
}

/// The creation of rskj's empty contract uses 64,857 gas before RSKIP438,
/// and 64,863 from it: 2 more for each of its init code's three words.
#[test]
fn a_creation_of_rskjs_empty_contract_uses_6_gas_more_from_rskip438() {
    let mainnet = RskHardforkConfig::mainnet;
    let test = "runCreate01ResourceBeforeActivationOfRSKIP438";
    assert_eq!(creation_gas(mainnet(), LOVELL - 1), 64_857, "{test}");
    assert_eq!(
        creation_gas(mainnet(), LOVELL),
        64_863,
        "runCreate01Resource"
    );
}

/// rskj's test turns RSKIP438 off by leaving lovell700 unscheduled on its
/// regtest, where every later upgrade is at block 0.
#[test]
#[ignore = "rskj leaves RSKIP438 off where lovell700 is unscheduled though a later upgrade is \
            scheduled (ActivationConfig.isActive reads lovell700's height), so a creation \
            pays nothing for its init code (Transaction.transactionCost); rustock runs revm's \
            SHANGHAI spec, with EIP-3860's price, from any upgrade past lovell700 \
            (hardfork.rs upgrade_to_spec_id)"]
fn a_creation_of_rskjs_empty_contract_uses_64857_gas_where_lovell700_is_unscheduled() {
    let regtest = RskHardforkConfig::rskj_regtest();
    let config = regtest.with_hardfork(RskNetworkUpgrade::Lovell700, None);
    let test = "runCreate01ResourceBeforeActivationOfRSKIP438";
    assert_eq!(creation_gas(config, 1), 64_857, "{test}");
}

/// rskj's test turns RSKIP174 off by leaving iris300 unscheduled on its
/// regtest, where every later upgrade is at block 0: a deployment onto an
/// address that holds 100 wei then leaves it none.
#[test]
#[ignore = "rskj leaves RSKIP174 off where iris300 is unscheduled though a later upgrade is \
            scheduled (ActivationConfig.isActive reads iris300's height), so a deployment writes \
            a new account over the balance at its address (TransactionExecutor.create); \
            rustock's has_rskip174 (hardfork.rs) is on from any upgrade past iris300, and the \
            balance stays"]
fn a_deployment_leaves_no_balance_at_its_address_where_iris300_is_unscheduled() {
    let regtest = RskHardforkConfig::rskj_regtest();
    let mut chain = Chain::new(regtest.with_hardfork(RskNetworkUpgrade::Iris300, None), 1);
    chain.put(SENDER, 0, ONE_RBTC, &[]);
    chain.put(FUNDER, 0, ONE_RBTC, &[]);
    let address = SENDER.create(0);
    let funded = chain.run(FUNDER, call(0, address, U256::from(100), GAS, &[]));
    assert!(funded.success);
    assert_eq!(chain.balance(address), U256::from(100));
    let created = chain.run(SENDER, create(0, 500_000, &empty_contract()));
    assert!(created.success);
    let test = "runCreateContractAndPreserveNoBalance";
    assert_eq!(chain.balance(address), U256::ZERO, "{test}");
}
