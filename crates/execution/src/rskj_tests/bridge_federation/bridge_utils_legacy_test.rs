//! rskj `co.rsk.peg.BridgeUtilsLegacyTest`: the utilities rskj ran before
//! RSKIP284, RSKIP293 and RSKIP271, through the Bridge method that reaches
//! them.

use alloy_primitives::Address as RskAddress;

use super::support::{self, flyover_code, FlyoverCall};
use crate::bridge::constants::BridgeConstants;
use crate::hardfork::RskHardforkConfig;

/// What `registerFastBridgeBtcTransaction` answers for the two Bitcoin
/// addresses `user` and `lp`, with a transaction it never gets to.
fn answer(net: (BridgeConstants, RskHardforkConfig), block: u64, user: &[u8], lp: &[u8]) -> i128 {
    let call = FlyoverCall {
        tx: &[1, 0, 0, 0, 1],
        height: i64::from(i32::MAX),
        derivation_arguments: [0x11; 32],
        user,
        lbc: RskAddress::repeat_byte(0x42),
        lp,
    };
    call.answer(&mut support::ctx_at(block), &net.0, &net.1)
}

/// rskj `BridgeUtilsLegacy.deserializeBtcAddressWithVersionLegacy`, which
/// `Bridge.registerFlyoverBtcTransaction` reads both its Bitcoin addresses
/// with before RSKIP284 (BridgeUtilsLegacy.java:59-79): no bytes throw, and
/// the hash is the 20 bytes after the version, which throws for fewer
/// (`System.arraycopy`). bitcoinj then refuses a version its network doesn't
/// take, and the method answers GENERIC_ERROR for each. The blocks are past
/// iris300 and before RSKIP284: testnet #2,122,593 and mainnet #4,000,000.
#[test]
fn before_rskip284_a_flyover_address_is_refused_short_or_of_another_network() {
    let testnet = || (BridgeConstants::testnet(), RskHardforkConfig::testnet());
    let mainnet = || (BridgeConstants::mainnet(), RskHardforkConfig::mainnet());
    let with = |version: u8| [&[version][..], &[0x41; 20][..]].concat();
    for (case, net, block, user, lp) in [
        ("no bytes", testnet(), 2_122_593, vec![], with(0x6f)),
        ("a hash of one byte", testnet(), 2_122_593, vec![0x6f, 0x41], with(0x6f)),
        ("a testnet p2sh version on mainnet", mainnet(), 4_000_000, with(0xc4), with(0x00)),
        ("a mainnet p2sh version on testnet", testnet(), 2_122_593, with(0x05), with(0x6f)),
    ] {
        assert_eq!(answer(net, block, &user, &lp), flyover_code::GENERIC_ERROR, "{case}");
    }
    // The same addresses on their own networks get past them.
    assert_ne!(
        answer(mainnet(), 4_000_000, &with(0x05), &with(0x00)),
        flyover_code::GENERIC_ERROR
    );
    assert_ne!(
        answer(testnet(), 2_122_593, &with(0x6f), &with(0x6f)),
        flyover_code::GENERIC_ERROR
    );
}
