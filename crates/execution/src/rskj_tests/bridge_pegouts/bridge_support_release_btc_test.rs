//! rskj's `BridgeSupportReleaseBtcTest`: here its tests of what RSKIP185
//! changes in `releaseBtc`. rskj's tests run `BridgeSupport` under
//! papyrus200 and under every rule; the ports run mainnet's Bridge at the
//! block before iris300, which brings RSKIP185, and at iris300.

use super::support::*;
use alloy_primitives::{Address, U256};
use revm::precompile::{PrecompileError, PrecompileOutput};

const BEFORE_RSKIP185: u64 = MAINNET_IRIS300 - 1;
const FROM_RSKIP185: u64 = MAINNET_IRIS300;

/// A request below every minimum, in satoshis.
const LOW: u64 = 1_000;

/// A contract that calls the Bridge.
const CONTRACT: Address = Address::repeat_byte(0xc0);

/// `satoshis` in wei.
fn wei(satoshis: u64) -> U256 {
    U256::from(satoshis) * U256::from(WEI_PER_SATOSHI)
}

/// The requests queued.
fn queued(bridge: &mut Bridge) -> usize {
    crate::bridge::peg::load_release_request_queue(&mut bridge.ctx, true).len()
}

/// An address as an indexed topic holds it.
fn as_topic(address: Address) -> [u8; 32] {
    let mut topic = [0u8; 32];
    topic[12..].copy_from_slice(address.as_slice());
    topic
}

/// `releaseBtc` of `satoshis` reached from [`CONTRACT`], a call from its
/// code, the value already at the Bridge.
fn release_from_a_contract(
    bridge: &mut Bridge,
    satoshis: u64,
) -> Result<PrecompileOutput, PrecompileError> {
    bridge.caller = CONTRACT;
    let bridge_address = crate::precompiles::BRIDGE_ADDR;
    let held = bridge.balance(bridge_address);
    bridge.set_balance(bridge_address, held + wei(satoshis));
    let tx = BridgeTxContext {
        rsk_sender: CONTRACT,
        ..bridge.tx.clone()
    };
    crate::bridge::peg::release_btc(
        &mut bridge.ctx,
        0,
        &bridge.constants,
        &bridge.cfg,
        &tx,
        CONTRACT,
        2,
        wei(satoshis),
    )
}

/// rskj's `release_after_papyrus_before_iris` and
/// `release_after_iris_before_fingerroot`: a request that is accepted is
/// queued on both sides, and from RSKIP185 logged, `release_request_received`.
#[test]
fn from_rskip185_an_accepted_request_is_logged() {
    let received = event_topic("release_request_received(address,bytes,uint256)");
    for (number, expected) in [(BEFORE_RSKIP185, vec![]), (FROM_RSKIP185, vec![received])] {
        let mut bridge = Bridge::mainnet_at(number);
        bridge.release(BTC);
        assert_eq!(queued(&mut bridge), 1, "at {number}");
        let logs = logs(&mut bridge.ctx);
        let firsts: Vec<[u8; 32]> = logs.iter().map(|log| topics(log)[0]).collect();
        assert_eq!(firsts, expected, "at {number}");
    }
}

/// rskj's `release_after_papyrus_before_iris_rejected_lowAmount` and
/// `release_after_iris_rejected_lowAmount`: a request below the minimum is
/// not queued. Before RSKIP185 the Bridge keeps its value and logs nothing;
/// from it the value goes back to the sender, and `release_request_rejected`
/// logs it with the reason LOW_AMOUNT. As in rskj's test, the fee per
/// kilobyte is 5,000 satoshis, which leaves the minimum the larger bound;
/// at mainnet's genesis fee per kilobyte the fee would be, and the reason
/// FEE_ABOVE_VALUE.
#[test]
fn from_rskip185_a_request_below_the_minimum_is_refunded_and_logged() {
    let rejected = event_topic("release_request_rejected(address,uint256,int256)");
    let bridge_address = crate::precompiles::BRIDGE_ADDR;
    let mut bridge = Bridge::mainnet_at(BEFORE_RSKIP185);
    bridge.release(LOW);
    assert_eq!(queued(&mut bridge), 0);
    assert_eq!(bridge.balance(STRANGER), U256::ZERO);
    assert_eq!(bridge.balance(bridge_address), wei(LOW));
    assert!(logs(&mut bridge.ctx).is_empty());

    let mut bridge = Bridge::mainnet_at(FROM_RSKIP185);
    put_named(
        &mut bridge.ctx,
        "feePerKb",
        &rlp_encode_element(&[0x13, 0x88]),
    );
    bridge.release(LOW);
    assert_eq!(queued(&mut bridge), 0);
    assert_eq!(bridge.balance(STRANGER), wei(LOW));
    assert_eq!(bridge.balance(bridge_address), U256::ZERO);
    let logs = logs(&mut bridge.ctx);
    assert_eq!(logs.len(), 1);
    assert_eq!(topics(&logs[0]), [rejected, as_topic(STRANGER)]);
    assert_eq!(word(&logs[0].data.data, 0), U256::from(LOW));
    assert_eq!(word(&logs[0].data.data, 1), U256::from(1), "LOW_AMOUNT");
}

/// From RSKIP271 rskj sizes the peg-out whose fee a request must cover by
/// the active federation's format (`BridgeUtils.calculatePegoutTxSize`): a
/// P2SH-P2WSH ERP federation's spend weighs its witness at a quarter, a P2SH
/// ERP federation's counts it whole. At 0.02 BTC a kilobyte, a request of
/// 0.03 BTC covers the first and not the second, which turns it down with
/// the reason FEE_ABOVE_VALUE.
#[test]
fn from_rskip271_a_segwit_federations_pegout_is_sized_by_its_weight() {
    let rejected = event_topic("release_request_rejected(address,uint256,int256)");
    for (format, accepted) in [(P2SH_ERP, false), (P2SH_P2WSH_ERP, true)] {
        let mut bridge = Bridge::mainnet_at(MAINNET_VETIVER900);
        store_federation(&mut bridge.ctx, NEW_FEDERATION, &active(), 1, format);
        put_named(
            &mut bridge.ctx,
            "feePerKb",
            &rlp_encode_element(&[0x1e, 0x84, 0x80]),
        );
        bridge.release(3 * CENT);
        assert_eq!(
            queued(&mut bridge),
            usize::from(accepted),
            "format {format}"
        );
        let logs = logs(&mut bridge.ctx);
        let turned_down: Vec<_> = logs
            .iter()
            .filter(|log| topics(log)[0] == rejected)
            .collect();
        if accepted {
            assert!(turned_down.is_empty());
        } else {
            assert_eq!(turned_down.len(), 1);
            assert_eq!(
                word(&turned_down[0].data.data, 1),
                U256::from(3),
                "FEE_ABOVE_VALUE"
            );
        }
    }
}

/// rskj's `release_after_papyrus_before_iris_rejected_contractCaller_throws_exception`
/// and `release_after_iris_rejected_contractCaller_emits_rejection_event`: a
/// contract's request fails the call before RSKIP185, rskj's
/// `OutOfGasException`; from it the call returns, the value is not sent back,
/// and `release_request_rejected` logs the contract with the reason
/// CALLER_CONTRACT.
#[test]
fn from_rskip185_a_contracts_request_is_logged_and_not_refunded() {
    let mut bridge = Bridge::mainnet_at(BEFORE_RSKIP185);
    assert!(matches!(
        release_from_a_contract(&mut bridge, BTC),
        Err(PrecompileError::OutOfGas)
    ));

    let rejected = event_topic("release_request_rejected(address,uint256,int256)");
    let mut bridge = Bridge::mainnet_at(FROM_RSKIP185);
    release_from_a_contract(&mut bridge, BTC).expect("releaseBtc answers");
    assert_eq!(queued(&mut bridge), 0);
    assert_eq!(bridge.balance(CONTRACT), U256::ZERO);
    assert_eq!(bridge.balance(crate::precompiles::BRIDGE_ADDR), wei(BTC));
    let logs = logs(&mut bridge.ctx);
    assert_eq!(logs.len(), 1);
    assert_eq!(topics(&logs[0]), [rejected, as_topic(CONTRACT)]);
    assert_eq!(word(&logs[0].data.data, 1), U256::from(2), "CALLER_CONTRACT");
}
