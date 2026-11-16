//! rskj's `BridgeSupportAddSignatureTest`: here its tests of when
//! `addSignature` logs `add_signature`, which RSKIP326 changes. Before it the
//! event is logged for each call of a federator's, before the signature is
//! checked; from it, only when a signature is added. rskj's tests mock
//! RSKIP146 on and RSKIP326 either way; the ports run mainnet's Bridge at
//! hop400, with RSKIP146 and before RSKIP326, and at fingerroot500, which
//! brings it.

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
