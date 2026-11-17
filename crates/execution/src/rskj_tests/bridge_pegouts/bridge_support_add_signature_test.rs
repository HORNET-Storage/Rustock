//! rskj's `BridgeSupportAddSignatureTest`: what `addSignature` signs in, and
//! when it logs `add_signature`, which RSKIP326 changes. Before it the event
//! is logged for each call of a federator's, before the signature is
//! checked; from it, only when a signature is added. rskj's tests mock
//! RSKIP146 on and RSKIP326 either way; the ports run mainnet's Bridge at
//! hop400, with RSKIP146 and before RSKIP326, and at fingerroot500, which
//! brings it. rskj's tests hand `BridgeSupport` what the Bridge would turn
//! down before it (no signatures, bytes that are not DER); the ports call
//! the Bridge, as a member's transaction does.

use super::support::*;

const BEFORE_RSKIP326: u64 = MAINNET_HOP400;
const FROM_RSKIP326: u64 = MAINNET_FINGERROOT500;

/// The DER signature of r and s both 1: it reads, and signs nothing.
const WRONG: &[u8] = &[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01];

/// How many `add_signature` events `calls` calls of the first member's log
/// at block `number`, each with its own signature, or with [`WRONG`].
fn add_signature_events(number: u64, valid: bool, calls: usize) -> usize {
    let mut signing = Signing::mainnet_at(number);
    let signature = if valid {
        signing.signature(0)
    } else {
        WRONG.to_vec()
    };
    for _ in 0..calls {
        signing
            .add_signature(0, &signature)
            .expect("addSignature runs");
    }
    let topic = event_topic("add_signature(bytes32,address,bytes)");
    logs(&mut signing.bridge.ctx)
        .iter()
        .filter(|log| topics(log)[0] == topic)
        .count()
}

/// rskj's `addSignature_calledTwiceWithSameFederatorPreRSKIP326_emitEventTwice`
/// and `addSignature_calledTwiceWithSameFederatorPostRSKIP326_emitEventOnlyOnce`.
#[test]
fn a_federator_signing_twice_is_logged_twice_before_rskip326_and_once_from_it() {
    assert_eq!(add_signature_events(BEFORE_RSKIP326, true, 2), 2);
    assert_eq!(add_signature_events(FROM_RSKIP326, true, 2), 1);
}

/// rskj's `addSignatureCreateEventLog_preRSKIP326` and
/// `addSignature_afterRSKIP326_eventEmitted`.
#[test]
fn a_federators_signature_is_logged() {
    assert_eq!(add_signature_events(BEFORE_RSKIP326, true, 1), 1);
    assert_eq!(add_signature_events(FROM_RSKIP326, true, 1), 1);
}

/// rskj's `addSignature_invalidSignatureBeforeRSKIP326_eventStillEmitted` and
/// `addSignature_invalidSignatureAfterRSKIP326_noEventEmitted`. rskj's tests
/// hand `BridgeSupport` bytes that aren't DER; the Bridge turns those down
/// before `BridgeSupport` sees them, so here a DER signature that doesn't
/// verify takes their place.
#[test]
fn a_signature_that_does_not_verify_is_logged_before_rskip326_only() {
    assert_eq!(add_signature_events(BEFORE_RSKIP326, false, 1), 1);
    assert_eq!(add_signature_events(FROM_RSKIP326, false, 1), 0);
}

/// The peg-out as it waits for signatures.
fn waiting(signing: &mut Signing) -> bitcoin::Transaction {
    let kept = crate::bridge::peg::deserialize_rsk_txs_waiting_for_signatures(
        &named(&mut signing.bridge.ctx, "rskTxsWaitingFS").unwrap_or_default(),
    );
    bitcoin::consensus::deserialize(&kept[&RELEASE_HASH]).expect("a transaction")
}

/// rskj's `addSignatureNonCanonicalSignature`: a member's signature with its
/// s in the upper half of the curve's order verifies, but rskj's
/// `getTransactionSignatures` wants it canonical (`isCanonical`) and signs
/// nothing.
#[test]
fn a_signature_whose_s_is_in_the_upper_half_of_the_order_signs_nothing() {
    let mut signing = Signing::mainnet_at(FROM_RSKIP326);
    let low = k256::ecdsa::Signature::from_der(&signing.signature(0)).expect("DER");
    let (r, s) = low.split_scalars();
    let high =
        k256::ecdsa::Signature::from_scalars(r.to_bytes(), (-s).to_bytes()).expect("a signature");
    signing
        .add_signature(0, high.to_der().as_bytes())
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.unsigned);
}

/// A member's signature with its r's length in DER's long form, which
/// bitcoinj's `decodeFromDER` reads (Bouncy Castle's `ASN1InputStream`), or
/// with a byte after it, which bitcoinj reads past: each is signed in as
/// bitcoinj writes the signature, `TransactionSignature.encodeToBitcoin`,
/// not as it was sent.
#[test]
fn a_signature_bitcoinj_reads_is_signed_in_as_bitcoinj_writes_it() {
    let der = Signing::mainnet_at(FROM_RSKIP326).signature(0);
    // `30 L 02 lr r 02 ls s`, with r's length written `81 lr`.
    let long_form = [&[0x30, der[1] + 1, 0x02, 0x81][..], &der[3..]].concat();
    let trailing = [der.clone(), vec![0x00]].concat();
    for sent in [long_form, trailing] {
        let mut signing = Signing::mainnet_at(FROM_RSKIP326);
        signing.add_signature(0, &sent).expect("addSignature runs");
        assert_eq!(waiting(&mut signing), signing.signed_by(&[0]));
    }
}

/// A signature with an integer whose first byte only repeats the sign of
/// the next: Bouncy Castle refuses the integer (`ASN1Integer.isMalformed`),
/// bitcoinj's `decodeFromDER` throws, and so, from RSKIP88, does the
/// Bridge's `addSignature`. A first byte the next one's sign needs is read,
/// and so is an integer with no bytes, as 0; neither signs anything.
#[test]
fn a_signature_with_an_integer_padded_past_its_sign_fails_the_call() {
    let mut signing = Signing::mainnet_at(FROM_RSKIP326);
    let padded: [&[u8]; 2] = [
        &[0x30, 0x07, 0x02, 0x02, 0x00, 0x7f, 0x02, 0x01, 0x01],
        &[0x30, 0x07, 0x02, 0x01, 0x01, 0x02, 0x02, 0xff, 0x80],
    ];
    for sent in padded {
        assert!(signing.add_signature(0, sent).is_err(), "{sent:02x?}");
    }
    let read: [&[u8]; 3] = [
        &[0x30, 0x07, 0x02, 0x02, 0x00, 0x80, 0x02, 0x01, 0x01],
        &[0x30, 0x07, 0x02, 0x01, 0x01, 0x02, 0x02, 0xff, 0x7f],
        &[0x30, 0x04, 0x02, 0x00, 0x02, 0x00],
    ];
    for sent in read {
        assert!(signing.add_signature(0, sent).is_ok(), "{sent:02x?}");
    }
    assert_eq!(waiting(&mut signing), signing.unsigned);
}

/// The peg-outs waiting for signatures, by the hash of the transaction that
/// made each.
fn waiting_for_signatures(signing: &mut Signing) -> std::collections::BTreeMap<[u8; 32], Vec<u8>> {
    crate::bridge::peg::deserialize_rsk_txs_waiting_for_signatures(
        &named(&mut signing.bridge.ctx, "rskTxsWaitingFS").unwrap_or_default(),
    )
    .into_iter()
    .collect()
}

/// The private key of no member.
const STRANGER_KEY: [u8; 32] = [0x77; 32];

/// rskj's `addSignatureToMissingTransaction`: a member's signature for a
/// peg-out that is not waiting for signatures changes nothing, and logs
/// nothing.
#[test]
fn a_signature_for_a_pegout_not_waiting_changes_nothing() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    let signature = signing.signature(0);
    put_named(
        &mut signing.bridge.ctx,
        "rskTxsWaitingFS",
        &rlp_encode_list(&[]),
    );
    signing
        .add_signature(0, &signature)
        .expect("addSignature runs");
    assert!(waiting_for_signatures(&mut signing).is_empty());
    assert!(logs(&mut signing.bridge.ctx).is_empty());
}

/// rskj's `addSignatureFromInvalidFederator`: a member's call with the key
/// of no member, and that key's signature, signs nothing and logs nothing.
#[test]
fn the_key_of_no_member_signs_nothing() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    let signature = signing.signature_by(&STRANGER_KEY, 0);
    signing
        .add_signatures_as(0, &public_key(&STRANGER_KEY), &[signature])
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.unsigned);
    assert!(logs(&mut signing.bridge.ctx).is_empty());
}

/// rskj's `addSignatureWithInvalidSignature`: a member's call with the
/// signature of a key that is no member's signs nothing.
#[test]
fn a_signature_by_another_key_signs_nothing() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    let signature = signing.signature_by(&STRANGER_KEY, 0);
    signing
        .add_signature(0, &signature)
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.unsigned);
}

/// rskj's `addSignatureWithLessSignaturesThanExpected`: no signatures for the
/// peg-out's one input. rskj's `BridgeSupport` returns; the Bridge turns an
/// empty list down before it, and from RSKIP88 fails the call.
#[test]
fn no_signatures_fail_the_call() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    assert!(signing.add_signatures(0, &[]).is_err());
    assert_eq!(waiting(&mut signing), signing.unsigned);
}

/// rskj's `addSignatureWithMoreSignaturesThanExpected`: two signatures for
/// the peg-out's one input sign nothing.
#[test]
fn more_signatures_than_inputs_sign_nothing() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    let signature = signing.signature(0);
    signing
        .add_signatures(0, &[signature.clone(), signature])
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.unsigned);
}

/// rskj's `addSignatureOneSignature`: one member's signature is kept, and
/// the peg-out waits for the rest.
#[test]
fn one_members_signature_is_kept() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    signing.sign(0);
    assert_eq!(waiting(&mut signing), signing.signed_by(&[0]));
}

/// rskj's `addSignatureTwice`: a member that signed signs nothing more with
/// another signature of its own.
#[test]
fn a_member_that_signed_signs_nothing_more() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    signing.sign(0);
    let another = signing.another_signature_of(0, 0);
    assert_ne!(another, signing.signature(0));
    signing
        .add_signature(0, &another)
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.signed_by(&[0]));
}

/// The raw transaction a `release_btc` event logs.
fn released(log: &revm::primitives::Log) -> Vec<u8> {
    let data = &log.data.data;
    let length = word(data, 1).to::<usize>();
    data[64..64 + length].to_vec()
}

/// rskj's `addSignatureTwoSignatures`: the second of the federation's two
/// needed signatures signs the peg-out. It no longer waits, and after each
/// member's `add_signature` the Bridge logs `release_btc` with it signed.
#[test]
fn two_members_sign_the_pegout_and_it_is_released() {
    let mut signing = Signing::mainnet_at(BEFORE_RSKIP326);
    signing.sign(0);
    signing.sign(1);
    assert!(waiting_for_signatures(&mut signing).is_empty());
    let logs = logs(&mut signing.bridge.ctx);
    let firsts: Vec<[u8; 32]> = logs.iter().map(|log| topics(log)[0]).collect();
    let add = event_topic("add_signature(bytes32,address,bytes)");
    let release = event_topic("release_btc(bytes32,bytes)");
    assert_eq!(firsts, [add, add, release]);
    assert_eq!(topics(&logs[2])[1], RELEASE_HASH);
    assert_eq!(
        released(&logs[2]),
        bitcoin::consensus::serialize(&signing.signed_by(&[0, 1]))
    );
}

/// rskj's `addSignatureMultipleInputsPartiallyValid`: a peg-out of three
/// inputs. A member's three signatures, the third by another key, sign none
/// of them; with a third that bitcoinj cannot read, the Bridge fails the
/// call, as rskj's test, which hands it to `BridgeSupport`, does not see;
/// with the member's own three, the three inputs are signed; and a second
/// member's three sign the peg-out, which is released.
#[test]
fn a_pegout_of_three_inputs_is_signed_only_by_signatures_for_all_three() {
    let mut signing = Signing::spending(BEFORE_RSKIP326, 3);
    let mut first: Vec<Vec<u8>> = (0..3).map(|index| signing.signature_of(0, index)).collect();
    let own = first[2].clone();
    first[2] = signing.signature_by(&STRANGER_KEY, 2);
    signing
        .add_signatures(0, &first)
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.unsigned);
    first[2] = (0..own.len() as u8).collect();
    assert!(signing.add_signatures(0, &first).is_err());
    assert_eq!(waiting(&mut signing), signing.unsigned);
    first[2] = own;
    signing
        .add_signatures(0, &first)
        .expect("addSignature runs");
    assert_eq!(waiting(&mut signing), signing.signed_by(&[0]));
    let second: Vec<Vec<u8>> = (0..3).map(|index| signing.signature_of(1, index)).collect();
    signing
        .add_signatures(1, &second)
        .expect("addSignature runs");
    assert!(waiting_for_signatures(&mut signing).is_empty());
    let logs = logs(&mut signing.bridge.ctx);
    let add = event_topic("add_signature(bytes32,address,bytes)");
    let release = event_topic("release_btc(bytes32,bytes)");
    let firsts: Vec<[u8; 32]> = logs.iter().map(|log| topics(log)[0]).collect();
    assert_eq!(firsts, [add, add, add, release]);
    assert_eq!(
        released(&logs[3]),
        bitcoin::consensus::serialize(&signing.signed_by(&[0, 1]))
    );
}
