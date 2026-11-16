//! rskj's `BridgeSupportTest.ReleaseTransactionInfo` tests of the refund of a
//! legacy peg-in from a multisig sender
//! (`registerBtcTransaction_forRefundableLegacyPeginFromMultisig_*RSKIP305*`):
//! from RSKIP428 the Bridge logs `pegout_transaction_created` with the value
//! of each output the refund spends, and from RSKIP305 it also keeps those
//! values under the refund's hash (`BridgeSupport.processReleaseTransactionInfo`).
//! Lovell700 is before RSKIP305, and Reed800 brings it; rskj's test of the
//! latter has every upgrade on, which Vetiver900 is.
//!
//! The peg-in pays mainnet's genesis federation, at the height the pegout
//! index is read from.

use alloy_primitives::{B256, U256};
use bitcoin::Transaction;

use crate::bridge::constants::BridgeConstants;
use crate::bridge::{storage, tx as bridge_tx};
use crate::hardfork::{RskNetworkUpgrade, RSK_MAINNET_CHAIN_ID};

use super::super::support::*;

use RskNetworkUpgrade::{Lovell700, Reed800, Vetiver900};

/// One bitcoin as bitcoinj's `VarInt` writes it.
const ONE_BITCOIN_VARINT: [u8; 5] = [0xfe, 0x00, 0xe1, 0xf5, 0x05];

/// `pegout_transaction_created(bytes32 indexed btcTxHash, bytes
/// utxoOutpointValues)`, the values as bitcoinj's `VarInt`s.
fn pegout_transaction_created(release: &Transaction, values: &[u8]) -> Event {
    let mut data = U256::from(32u64).to_be_bytes::<32>().to_vec();
    data.extend_from_slice(&U256::from(values.len()).to_be_bytes::<32>());
    data.extend_from_slice(values);
    data.resize(data.len().div_ceil(32) * 32, 0);
    (
        vec![
            topic("pegout_transaction_created(bytes32,bytes)"),
            B256::from(txid_bytes(release)),
        ],
        data,
    )
}

/// A bitcoin from a two-of-two multisig to mainnet's federation, registered
/// at RSK block 10 of mainnet's ladder up to `last`: its refund, the events
/// the registration logged, and what is kept under the refund's hash.
fn refunded(last: RskNetworkUpgrade) -> (Transaction, Vec<Event>, Option<Vec<u8>>) {
    let config = BridgeConstants::mainnet();
    let cfg = up_to(last, RSK_MAINNET_CHAIN_ID);
    let redeem = multisig(
        &[
            key_from_seed("legacy_pegin_p2sh_multisig_key_1"),
            key_from_seed("legacy_pegin_p2sh_multisig_key_2"),
        ],
        2,
    );
    let pegin = tx(
        vec![p2sh_multisig_spend(&redeem, 2)],
        &[(COIN, genesis_federation_output(&config))],
    );
    let height = config.btc_height_when_pegout_tx_index_activates
        + config.pegout_tx_index_grace_period_in_btc_blocks;
    let mut ctx = bridge_holding(&pegin, height, 10, &config, &cfg);
    let registered = register(&mut ctx, &pegin, i64::from(height), &config, &cfg);
    assert!(registered.processed);
    let refunds = waiting_pegouts(&mut ctx);
    assert_eq!(refunds.len(), 1, "a refund");
    let refund = refunds[0].clone();
    let key = storage::compound_key(
        storage::RELEASES_OUTPOINTS_VALUES_KEY,
        "-",
        &bridge_tx::to_hex(&txid_bytes(&refund)),
    );
    let kept = storage::bridge_load_raw(&mut ctx, key);
    (refund, registered.logs, kept)
}

/// `registerBtcTransaction_forRefundableLegacyPeginFromMultisig_beforeRSKIP305_justEmitsPegoutTransactionCreatedEvent`.
#[test]
fn before_rskip305_a_refund_is_logged_with_what_it_spends_and_nothing_is_kept() {
    let (refund, logs, kept) = refunded(Lovell700);
    assert!(logs.contains(&release_requested(&refund, COIN)), "{logs:?}");
    assert!(
        logs.contains(&pegout_transaction_created(&refund, &ONE_BITCOIN_VARINT)),
        "{logs:?}"
    );
    assert_eq!(kept, None);
}

/// `registerBtcTransaction_forRefundableLegacyPeginFromMultisig_afterRSKIP305_processReleaseTransactionsInfo`.
#[test]
fn from_rskip305_what_a_refund_spends_is_kept_under_its_hash() {
    for last in [Reed800, Vetiver900] {
        let (refund, logs, kept) = refunded(last);
        assert!(
            logs.contains(&release_requested(&refund, COIN)),
            "{last:?} {logs:?}"
        );
        let created = pegout_transaction_created(&refund, &ONE_BITCOIN_VARINT);
        assert!(logs.contains(&created), "{last:?} {logs:?}");
        assert_eq!(kept, Some(ONE_BITCOIN_VARINT.to_vec()), "{last:?}");
    }
}
