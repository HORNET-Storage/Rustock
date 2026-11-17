//! A federator's signature as bitcoinj-thin's
//! `BtcECKey.ECDSASignature.decodeFromDER` reads it.
//!
//! rskj reads every signature `addSignature` is given this way before it does
//! anything else, and fails the call where it throws (`Bridge.addSignature`);
//! then it reads each again to sign with it
//! (`BridgeSupport.getDecodedSignatures`).

/// Bouncy Castle `ASN1InputStream.readLength` of the length at `at`: the
/// short form, or a long form of one to four bytes. The indefinite form is
/// refused here, as the BER sequence it gives is one bitcoinj's cast to a DL
/// sequence refuses, and a primitive Bouncy Castle refuses. Returns the
/// length and where what it measures starts; what measures past the bytes
/// there are, a Java `int`'s overflow among it, the caller refuses.
fn asn1_length(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let first = *bytes.get(at)?;
    if first < 0x80 {
        return Some((usize::from(first), at + 1));
    }
    let size = usize::from(first & 0x7f);
    if size == 0 || size > 4 {
        return None;
    }
    let mut length: u64 = 0;
    for k in 1..=size {
        length = (length << 8) | u64::from(*bytes.get(at + k)?);
    }
    let length = usize::try_from(length).ok()?;
    Some((length, at + 1 + size))
}

/// The universal tag `tag` at `at`, in either of the two ways Bouncy Castle's
/// `readTagNumber` reads it: the number in the tag byte's own five bits, or,
/// after a tag byte whose five bits are all set, in the byte that follows.
/// Returns where the length starts.
fn asn1_tag(bytes: &[u8], at: usize, tag: u8) -> Option<usize> {
    let first = *bytes.get(at)?;
    if first == tag {
        return Some(at + 1);
    }
    (first == (tag | 0x1f) && bytes.get(at + 1) == Some(&(tag & 0x1f))).then_some(at + 2)
}

/// The INTEGER at `at`, ending by `end`: its bytes, and where the next
/// element starts. Bouncy Castle refuses an integer whose first byte only
/// repeats the sign of the next (`ASN1Integer.isMalformed`): `00` before a
/// byte under `0x80`, `ff` before one of `0x80` or more. An integer with no
/// bytes it reads, as 0.
fn der_integer(sig: &[u8], at: usize, end: usize) -> Option<(&[u8], usize)> {
    let (length, start) = asn1_length(sig, asn1_tag(sig, at, 0x02)?)?;
    let stop = start.checked_add(length).filter(|&stop| stop <= end)?;
    let bytes = &sig[start..stop];
    let malformed = match bytes {
        [first, next, ..] => (*first == 0x00 && *next < 0x80) || (*first == 0xff && *next >= 0x80),
        _ => false,
    };
    (!malformed).then_some((bytes, stop))
}

/// The two integers of a signature as bitcoinj's
/// `BtcECKey.ECDSASignature.decodeFromDER` reads them, or `None` where it
/// throws. bitcoinj parses with the ASN.1 classes of Bouncy Castle 1.58, which
/// rskj ships under SpongyCastle's package names: `ASN1InputStream` reads one
/// object, which must be a SEQUENCE of a definite length, and nothing after
/// it; bitcoinj takes the sequence's first two elements as INTEGERs and reads
/// each as the unsigned number its bytes spell (`getPositiveValue`), so a
/// negative encoding is read as a positive number, not refused.
///
/// Bouncy Castle also parses whatever follows the two integers inside the
/// sequence, and throws where that is not ASN.1 it reads; here it is not
/// looked at.
pub(crate) fn der_signature_integers(sig: &[u8]) -> Option<(&[u8], &[u8])> {
    let after_tag = asn1_tag(sig, 0, 0x30)?;
    let (length, start) = asn1_length(sig, after_tag)?;
    let end = start.checked_add(length).filter(|&end| end <= sig.len())?;
    let (r, next) = der_integer(sig, start, end)?;
    let (s, _) = der_integer(sig, next, end)?;
    Some((r, s))
}
