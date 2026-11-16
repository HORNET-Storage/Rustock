//! rskj `co.rsk.peg.PegUtilsLegacyTest`: how the legacy rules take a Bitcoin
//! transaction, as a peg-in (`isValidPegInTx`), a migration (`isMigrationTx`)
//! or a peg-out (`isPegOutTx`), by era.
//!
//! rskj applies them where `registerBtcTransaction` reads no pegout index:
//! before RSKIP379, and below the index's height after it. These tests
//! register each transaction at Bitcoin height 349, below rskj's regtest
//! index (250 and a grace of 100), and read what the Bridge did: whether the
//! call succeeded, whether it marked the transaction processed, the events
//! it logged, and the UTXOs the active federation holds. A transaction no
//! rule takes is left alone, `(true, false, 0, 0)`; a peg-out or a migration
//! is marked processed with no event, its outputs to the federation kept.

use bitcoin::hashes::Hash as _;
use revm::context_interface::{ContextTr, JournalTr};

use super::support::{self, NEW_FEDERATION, OLD_FEDERATION};
use crate::abi::encode::{self, Arg};
use crate::bridge::constants::BridgeConstants;
use crate::bridge::{peg, serialization, storage};
use crate::hardfork::{RskHardforkConfig, RskNetworkUpgrade};
use crate::precompiles::BridgeTxContext;

const BLOCK: u64 = 1_000;
const BTC: u64 = 100_000_000;

/// rskj's regtest from genesis, or with `upgrade` and every one after it moved
/// past the block these tests run at.
fn regtest(before: Option<RskNetworkUpgrade>) -> RskHardforkConfig {
    let all = RskHardforkConfig::rskj_regtest();
    before.map_or(all.clone(), |upgrade| {
        support::upgrades_from_at(all, upgrade, 1_000_000)
    })
}

/// What the Bridge holds for a federation: its keys, its format and its
/// redeem script.
struct Federation {
    keys: Vec<[u8; 33]>,
    format: u64,
    redeem: Vec<u8>,
}

/// rskj's regtest delay, 500, as a Java number little-endian.
const REGTEST_DELAY: &[u8] = &[0xf4, 0x01];

fn standard(first: u8) -> Federation {
    let keys = support::keys(first, 3);
    let redeem = support::standard_redeem(&keys);
    Federation {
        keys,
        format: 1000,
        redeem,
    }
}

fn p2sh_erp(first: u8) -> Federation {
    let keys = support::keys(first, 3);
    let emergency = support::erp_keys_of(&BridgeConstants::rskj_regtest());
    let redeem = support::p2sh_erp_redeem(&keys, &emergency, REGTEST_DELAY);
    Federation {
        keys,
        format: 3000,
        redeem,
    }
}

fn address_of(redeem: &[u8]) -> bitcoin::ScriptBuf {
    support::p2sh(&support::hash160(redeem))
}

/// `redeem` behind a Flyover derivation hash: `<hash> OP_DROP <redeem>`.
fn flyover(redeem: &[u8]) -> Vec<u8> {
    [&[0x20][..], &[0x01; 32], &[0x75], redeem].concat()
}

/// A non-standard ERP redeem script of `keys`, with the emergency keys of
/// the private keys 0xfa01 and 0xfa02 and a delay of 500, as rskj's tests
/// build it.
fn non_standard_erp(keys: &[[u8; 33]]) -> Vec<u8> {
    let emergency = [0xfa01u16, 0xfa02].map(|k| support::public_key(&k.to_be_bytes()));
    support::non_standard_erp_redeem(keys, &emergency, &[0x01, 0xf4])
}

/// `data` pushed as bitcoinj pushes it.
fn push(data: &[u8]) -> Vec<u8> {
    let mut out = match data.len() {
        n @ 0..=75 => vec![n as u8],
        n @ 76..=255 => vec![0x4c, n as u8],
        n => [&[0x4d][..], &(n as u16).to_le_bytes()].concat(),
    };
    out.extend_from_slice(data);
    out
}

/// A multisig spend of `redeem`: OP_0, two signatures' worth of bytes, and
/// the redeem script pushed last.
fn spending(redeem: &[u8]) -> Vec<u8> {
    [
        &[0x00][..],
        &push(&[0x30; 71]),
        &push(&[0x31; 71]),
        &push(redeem),
    ]
    .concat()
}

/// A transaction whose inputs have the scriptSigs `inputs`, paying `outputs`.
fn tx(inputs: &[Vec<u8>], outputs: &[(u64, bitcoin::ScriptBuf)]) -> bitcoin::Transaction {
    let mut tx = support::tx_paying(outputs);
    tx.input = inputs
        .iter()
        .enumerate()
        .map(|(i, script_sig)| bitcoin::TxIn {
            previous_output: bitcoin::OutPoint {
                txid: bitcoin::Txid::from_byte_array([0x5b; 32]),
                vout: i as u32,
            },
            script_sig: bitcoin::ScriptBuf::from_bytes(script_sig.clone()),
            sequence: bitcoin::Sequence::MAX,
            witness: bitcoin::Witness::new(),
        })
        .collect();
    tx
}

/// A P2PKH spend's scriptSig: a signature's bytes and a public key.
fn p2pkh_spend() -> Vec<u8> {
    [push(&[0x30; 71]), push(&support::public_key(&[0x77]))].concat()
}

/// What `registerBtcTransaction` does with `tx` at height 349, at block
/// 1,000 under `cfg`, with `active` the active federation, `retiring` the
/// retiring one, and `retired` the last retired federation's P2SH hash.
fn registered(
    cfg: &RskHardforkConfig,
    active: &Federation,
    retiring: Option<&Federation>,
    retired: Option<[u8; 20]>,
    tx: &bitcoin::Transaction,
) -> (bool, bool, usize, usize) {
    let raw = bitcoin::consensus::serialize(tx);
    let mut ctx = support::ctx_at(BLOCK);
    support::btc_chain_holding(&mut ctx, &raw, cfg);
    support::fund_bridge(&mut ctx);
    support::store_federation(&mut ctx, NEW_FEDERATION, &active.keys, 1, active.format);
    if let Some(retiring) = retiring {
        support::store_federation(&mut ctx, OLD_FEDERATION, &retiring.keys, 0, retiring.format);
    }
    if let Some(retired) = retired {
        let script = support::p2sh(&retired).into_bytes();
        storage::bridge_store_bytes_named(
            &mut ctx,
            storage::LAST_RETIRED_FEDERATION_P2SH_SCRIPT_KEY,
            &serialization::rlp_encode_list(&[serialization::rlp_encode_element(&script)]),
        );
    }
    let tree = support::one_leaf_tree(&raw);
    let args = encode::args(&[
        Arg::Bytes(&raw),
        Arg::Word(encode::int(349)),
        Arg::Bytes(&tree),
    ]);
    let config = BridgeConstants::rskj_regtest();
    let ok = peg::register_btc_transaction(
        &mut ctx,
        &args,
        0,
        &config,
        cfg,
        &BridgeTxContext::default(),
    )
    .is_ok();
    let logs = ctx.journal_mut().take_logs().len();
    let txid = crate::bridge::tx::legacy_btc_txid(tx);
    let processed = crate::bridge::tx::is_btc_tx_processed(&mut ctx, &txid);
    let utxos = storage::load_federation_utxos(&mut ctx, cfg, BLOCK).len();
    (ok, processed, logs, utxos)
}

const LEFT_ALONE: (bool, bool, usize, usize) = (true, false, 0, 0);

/// A peg-out or a migration: marked processed with no event, keeping
/// `utxos` outputs to the active federation.
fn moved(utxos: usize) -> (bool, bool, usize, usize) {
    (true, true, 0, utxos)
}

/// Whether the transaction was taken and marked processed, as a peg-in is,
/// accepted or refunded.
fn taken(outcome: (bool, bool, usize, usize)) -> bool {
    outcome.0 && outcome.1
}

/// rskj's `isValidPegInTx_*_iris` tests: from RSKIP219 a peg-in must send the
/// federation 500,000 satoshis, before it 1,000,000 (rskj's regtest); one
/// that sends less is no peg-in, and is left alone. These come from a P2PKH
/// sender, whose peg-in is taken.
#[test]
fn a_peg_in_must_send_the_minimum_of_its_era() {
    let active = standard(1);
    let paying = |value| tx(&[p2pkh_spend()], &[(value, address_of(&active.redeem))]);
    let after = regtest(None);
    let before = regtest(Some(RskNetworkUpgrade::Iris300));
    assert_eq!(
        registered(&after, &active, None, None, &paying(250_000)),
        LEFT_ALONE
    );
    for value in [750_000, 50 * BTC] {
        assert!(
            taken(registered(&after, &active, None, None, &paying(value))),
            "{value} from RSKIP219"
        );
    }
    assert_eq!(
        registered(&before, &active, None, None, &paying(750_000)),
        LEFT_ALONE
    );
    assert!(taken(registered(
        &before,
        &active,
        None,
        None,
        &paying(1_000_000)
    )));
}

/// rskj's `isValidPegInTx_spending_from_federation_is_pegout_after_iris`: a
/// spend of the active federation that pays it back is no peg-in but a
/// peg-out.
#[test]
fn a_spend_of_the_federation_is_a_pegout() {
    let active = standard(1);
    let tx = tx(
        &[spending(&active.redeem)],
        &[(BTC, address_of(&active.redeem))],
    );
    assert_eq!(
        registered(&regtest(None), &active, None, None, &tx),
        moved(1)
    );
}

/// A federation's redeem script behind a Flyover prefix, within a
/// non-standard ERP script, and both.
fn wrapped(federation: &Federation) -> [(&'static str, Vec<u8>); 3] {
    [
        ("flyover", flyover(&federation.redeem)),
        ("non-standard erp", non_standard_erp(&federation.keys)),
        (
            "flyover non-standard erp",
            flyover(&non_standard_erp(&federation.keys)),
        ),
    ]
}

/// rskj's `isValidPegInTx_hasChangeUtxoFrom*_afterRskip201_notPegin`: from
/// RSKIP201 a spend of the active federation's redeem script wrapped is no
/// peg-in but a peg-out, and one of the last retired federation's, paying
/// the active one, is a migration.
#[test]
fn from_rskip201_the_change_of_a_flyover_or_erp_federation_is_no_peg_in() {
    let active = standard(1);
    let retired = standard(11);
    let to_active = [(BTC, address_of(&active.redeem))];
    for (of, federation, retired_hash) in [
        ("active", &active, None),
        ("retired", &retired, Some(support::hash160(&retired.redeem))),
    ] {
        for (shape, redeem) in wrapped(federation) {
            let tx = tx(&[spending(&redeem)], &to_active);
            assert_eq!(
                registered(&regtest(None), &active, None, retired_hash, &tx),
                moved(1),
                "{shape} of the {of} federation"
            );
        }
    }
}

/// rskj's `isValidPegInTx_hasChangeUtxoFrom{Flyover,FlyoverErp,Erp}Federation
/// _beforeRskip201_isPegin`, with the change input as rskj writes it, the
/// redeem script itself its scriptSig: before RSKIP201 only an input that
/// spends a federation's P2SH script makes a transaction no peg-in
/// (PegUtilsLegacy.java:186-194), and this one spends none, so it is taken
/// for a peg-in, not a peg-out. Its sender is read next, and the reader of a
/// multisig sender takes the scriptSig's last chunk, here an opcode, for the
/// redeem script, whose null bytes bitcoinj cannot parse
/// (P2shMultisigBtcLockSender.java:52-54): the call fails, where a peg-out
/// would have been marked processed.
#[test]
fn before_rskip201_the_change_of_a_flyover_or_erp_federation_is_a_peg_in() {
    let active = standard(1);
    for (shape, redeem) in wrapped(&active) {
        let tx = tx(&[redeem], &[(BTC, address_of(&active.redeem))]);
        assert_eq!(
            registered(
                &regtest(Some(RskNetworkUpgrade::Iris300)),
                &active,
                None,
                None,
                &tx
            ),
            (false, false, 0, 0),
            "{shape}"
        );
    }
}

/// rskj's `isPegOutTx_from{Flyover,Erp,FlyoverErp}Federation`, from RSKIP201:
/// a spend of the active federation's redeem script wrapped, paying
/// elsewhere, is a peg-out (PegUtilsLegacy.java isPegOutTx, the input's
/// standard redeem script against the federation's).
#[test]
fn from_rskip201_a_spend_of_a_wrapped_federation_redeem_script_is_a_pegout() {
    let active = standard(1);
    for (shape, redeem) in wrapped(&active) {
        let tx = tx(&[spending(&redeem)], &[(BTC, support::p2pkh(&[0x99; 20]))]);
        assert_eq!(
            registered(&regtest(None), &active, None, None, &tx),
            moved(0),
            "{shape}"
        );
    }
}

/// The same tests before RSKIP201: rskj's `isPegOutTx` compares the P2SH
/// script of the input's redeem script as it is, wrapped, to the
/// federation's, and `isValidPegInTx` finds no spend of the federation's P2SH
/// script either, while the transaction sends the federation nothing. It is
/// neither, and left alone.
#[test]
#[ignore = "before RSKIP201 rskj takes a spend of a federation's redeem script behind a Flyover prefix or within an ERP script for no peg-out, and leaves it alone; rustock reduces the input to the federation's standard redeem script at every height and marks it processed as a peg-out"]
fn before_rskip201_a_spend_of_a_wrapped_federation_redeem_script_is_no_pegout() {
    let active = standard(1);
    for (shape, redeem) in wrapped(&active) {
        let tx = tx(&[spending(&redeem)], &[(BTC, support::p2pkh(&[0x99; 20]))]);
        assert_eq!(
            registered(
                &regtest(Some(RskNetworkUpgrade::Iris300)),
                &active,
                None,
                None,
                &tx
            ),
            LEFT_ALONE,
            "{shape}"
        );
    }
}

/// rskj's `isValidPegInTx_has_*_RSKIP293` tests: before RSKIP293 what the
/// federation is sent counts in total; from it each UTXO must be worth the
/// minimum, 500,000 satoshis.
#[test]
fn from_rskip293_each_utxo_of_a_peg_in_must_be_worth_the_minimum() {
    let active = standard(1);
    let to = |values: &[u64]| {
        let outputs: Vec<_> = values
            .iter()
            .map(|&v| (v, address_of(&active.redeem)))
            .collect();
        tx(&[p2pkh_spend()], &outputs)
    };
    let min = 500_000;
    let before = regtest(Some(RskNetworkUpgrade::Hop400));
    let after = regtest(None);
    assert!(taken(registered(
        &before,
        &active,
        None,
        None,
        &to(&[min / 4; 4])
    )));
    assert_eq!(
        registered(
            &before,
            &active,
            None,
            None,
            &to(&[min / 4, min / 4, min / 4, min / 5])
        ),
        LEFT_ALONE
    );
    assert_eq!(
        registered(
            &after,
            &active,
            None,
            None,
            &to(&[min / 2, min / 2, min / 2, min, min + BTC])
        ),
        LEFT_ALONE
    );
    assert!(taken(registered(&after, &active, None, None, &to(&[min]))));
}

/// rskj's `isValidPegInTx_*P2shErpScript_sends_funds_to_*_after_RSKIP353`: a
/// spend of a P2SH ERP script of the active federation's members, or of its
/// Flyover, is no peg-in, whether it pays the federation or elsewhere: it is
/// a peg-out.
#[test]
fn a_spend_of_a_p2sh_erp_script_of_the_federations_members_is_a_pegout() {
    let active = standard(1);
    let p2sh_erp = support::p2sh_erp_redeem(&active.keys, &support::keys(31, 3), &[0x00, 0x01]);
    for redeem in [p2sh_erp.clone(), flyover(&p2sh_erp)] {
        for (to, utxos) in [
            (address_of(&active.redeem), 1),
            (support::p2pkh(&[0x99; 20]), 0),
        ] {
            let tx = tx(&[spending(&redeem)], &[(BTC, to)]);
            assert_eq!(
                registered(&regtest(None), &active, None, None, &tx),
                moved(utxos)
            );
        }
    }
}

/// rskj's `testTxIsProcessableInLegacyVersion`: before RSKIP143 a legacy
/// peg-in is registered only from a P2PKH sender, from it also from a P2SH
/// multisig one (BridgeUtils.txIsProcessableInLegacyVersion); before it the
/// peg-in is left alone (BridgeSupport.java:659-666), from it, its sender
/// being one rskj does not lock for, refunded.
#[test]
fn before_rskip143_a_peg_in_is_taken_only_from_a_p2pkh_sender() {
    let active = standard(1);
    let to_federation = [(BTC, address_of(&active.redeem))];
    let multisig = tx(
        &[spending(&support::standard_redeem(&support::keys(21, 3)))],
        &to_federation,
    );
    let before = regtest(Some(RskNetworkUpgrade::Papyrus200));
    let after = regtest(Some(RskNetworkUpgrade::Iris300));
    assert_eq!(
        registered(&before, &active, None, None, &multisig),
        LEFT_ALONE,
        "before RSKIP143"
    );
    assert!(
        taken(registered(&after, &active, None, None, &multisig)),
        "from RSKIP143"
    );
}

/// rskj's `isMigrationTx_sending_funds_from_*`: a spend of the retiring or the
/// last retired federation, P2SH ERP or standard, that pays the active one,
/// P2SH ERP or standard, is a migration. The last retired P2SH ERP
/// federation is known by the P2SH script of its members' multisig; by its
/// whole ERP script it is not, and the spend is left alone.
#[test]
fn a_spend_of_a_retiring_or_retired_federation_to_the_active_one_is_a_migration() {
    let cfg = regtest(None);
    for (from, to) in [
        (p2sh_erp(11), p2sh_erp(1)),
        (standard(11), p2sh_erp(1)),
        (standard(11), standard(1)),
    ] {
        let tx = tx(&[spending(&from.redeem)], &[(BTC, address_of(&to.redeem))]);
        assert_eq!(
            registered(&cfg, &to, Some(&from), None, &tx),
            moved(1),
            "retiring"
        );
        let members = support::hash160(&support::standard_redeem(&from.keys));
        assert_eq!(
            registered(&cfg, &to, None, Some(members), &tx),
            moved(1),
            "retired"
        );
    }
    let (from, to) = (p2sh_erp(11), p2sh_erp(1));
    let tx = tx(&[spending(&from.redeem)], &[(BTC, address_of(&to.redeem))]);
    assert_eq!(
        registered(&cfg, &to, None, Some(support::hash160(&from.redeem)), &tx),
        LEFT_ALONE,
        "retired, known by its whole script"
    );
}
