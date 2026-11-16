//! rskj's `BridgeSupportSigHashTest`: from RSKIP379 (arrowhead600) the
//! sighash of the first input of each peg-out `updateCollections` builds, a
//! batch or a migration, goes to the pegout index; before it, under
//! fingerroot500, none does. The ports run mainnet's Bridge at a block of
//! each and read the index.

use super::support::*;

const ERAS: [(u64, bool); 2] = [(MAINNET_FINGERROOT500, false), (MAINNET_ARROWHEAD600, true)];

/// The redeem script the first input of a peg-out the Bridge built spends:
/// the last push of its scriptSig.
fn spent_redeem(tx: &bitcoin::Transaction) -> Vec<u8> {
    match tx.input[0].script_sig.instructions().last() {
        Some(Ok(bitcoin::script::Instruction::PushBytes(bytes))) => bytes.as_bytes().to_vec(),
        other => panic!("no redeem script pushed last: {other:?}"),
    }
}

/// rskj's `test_pegoutTxIndex_when_pegout_batch_is_created`.
#[test]
fn from_rskip379_a_pegout_batch_built_goes_to_the_pegout_index() {
    for (number, rskip379) in ERAS {
        let mut collections = Collections::mainnet_at(number);
        collections.active(1, 10);
        collections.requests(3);
        collections.update();
        let waiting = collections.waiting();
        assert_eq!(waiting.len(), 1, "{number}");
        let redeem = standard_redeem(&active());
        assert_eq!(
            collections.indexed(&waiting[0], &redeem),
            rskip379,
            "{number}"
        );
    }
}

/// rskj's `test_pegoutTxIndex_when_migration_tx_is_created`.
#[test]
fn from_rskip379_a_migration_built_goes_to_the_pegout_index() {
    for (number, rskip379) in ERAS {
        let mut collections = Collections::migrating(number, STANDARD_MULTISIG, 10, false);
        collections.update();
        let waiting = collections.waiting();
        assert_eq!(waiting.len(), 1, "{number}");
        let redeem = standard_redeem(&retiring());
        assert_eq!(
            collections.indexed(&waiting[0], &redeem),
            rskip379,
            "{number}"
        );
    }
}

/// rskj's `test_pegoutTxIndex_when_migration_and_pegout_batch_tx_are_created`.
#[test]
fn from_rskip379_a_migration_and_a_pegout_batch_built_together_both_go_to_the_pegout_index() {
    for (number, rskip379) in ERAS {
        let mut collections = Collections::migrating(number, STANDARD_MULTISIG, 10, false);
        collections.requests(3);
        collections.update();
        let waiting = collections.waiting();
        assert_eq!(waiting.len(), 2, "{number}");
        let redeems: Vec<Vec<u8>> = waiting.iter().map(spent_redeem).collect();
        assert_ne!(redeems[0], redeems[1], "a migration and a batch");
        for (tx, redeem) in waiting.iter().zip(&redeems) {
            assert_eq!(collections.indexed(tx, redeem), rskip379, "{number}");
        }
    }
}
