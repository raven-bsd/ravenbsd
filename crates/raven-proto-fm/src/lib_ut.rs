// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the FMA ereport schema and ring.

use super::*;

fn sample() -> Ereport {
    Ereport::new(
        "ereport.io.mmio.bad",
        Severity::Major,
        "pci0:1:0",
        &[0xde, 0xad, 0xbe, 0xef],
        1_234,
    )
    .expect("sample")
}

#[test]
fn ereport_round_trips_exactly() {
    let event = sample();
    let mut bytes = [0u8; EREPORT_LEN];
    event.encode(&mut bytes).expect("encode");
    let decoded = Ereport::decode(&bytes).expect("decode");
    assert_eq!(decoded, event);
    assert_eq!(decoded.payload(), &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(decoded.class.as_str(), Some("ereport.io.mmio.bad"));
    assert_eq!(decoded.resource.as_str(), Some("pci0:1:0"));
}

#[test]
fn malformed_inputs_are_rejected() {
    let event = sample();
    let mut bytes = [0u8; EREPORT_LEN];
    event.encode(&mut bytes).expect("encode");

    assert_eq!(
        Ereport::decode(&bytes[..EREPORT_LEN - 1]),
        Err(FmError::BadLength)
    );
    let mut bad = bytes;
    bad[0] ^= 0xff;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadMagic));
    let mut bad = bytes;
    bad[2] = 9;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadVersion));
    let mut bad = bytes;
    bad[3] = 9;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadSeverity));
    let mut bad = bytes;
    bad[4] = 0x1f;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadString));
    let mut bad = bytes;
    bad[68] = PAYLOAD_LEN as u8 + 1;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadPayload));
    let mut bad = bytes;
    bad[143] = 1;
    assert_eq!(Ereport::decode(&bad), Err(FmError::BadReserved));
}

#[test]
fn ring_is_bounded_and_restartable() {
    let mut ring = EreportRing::<2>::new();
    assert!(ring.is_empty());
    let event = sample();
    ring.push(&event).expect("first");
    ring.push(&event).expect("second");
    assert_eq!(ring.push(&event), Err(FmError::Full));
    assert_eq!(ring.pop().expect("pop1"), event);
    assert_eq!(ring.pop().expect("pop2"), event);
    assert_eq!(ring.pop(), Err(FmError::Empty));
    ring.push(&event).expect("after restart");
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.pop().expect("restart pop"), event);
}

#[test]
fn text_fields_keep_a_terminating_nul() {
    let longest = [b'a'; CLASS_LEN - 1];
    assert!(Str32::new(core::str::from_utf8(&longest).expect("ascii")).is_ok());
    let too_long = [b'a'; CLASS_LEN];
    assert_eq!(
        Str32::new(core::str::from_utf8(&too_long).expect("ascii")),
        Err(FmError::BadString)
    );
}

#[test]
fn oversized_payload_is_rejected() {
    assert_eq!(
        Ereport::new("x", Severity::Minor, "r", &[0; PAYLOAD_LEN + 1], 0),
        Err(FmError::BadPayload)
    );
    assert!(Ereport::new("x", Severity::Minor, "r", &[], 0).is_ok());
}
