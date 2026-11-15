// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions over the model.

use super::*;

/// A synthetic layout; the real RP1 mailbox encoding is UNVERIFIED.
const LAYOUT: Layout = Layout {
    magic: 0x5A,
    magic_mask: 0xFF,
    version: 2,
    version_shift: 8,
    version_mask: 0xF,
    sequence_shift: 16,
    sequence_mask: 0xFF,
    length_shift: 24,
    length_mask: 0xF,
    status_shift: 12,
    status_mask: 0xF,
    checksum: true,
};

const DIGEST: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

#[test]
fn a_multi_chunk_image_drains_to_ready() {
    let plan = TransferPlan::new(0x2_4000, 0x4000).expect("plan");
    assert_eq!(plan.chunks(), 9);
    let digest = Digest::from_hex(DIGEST).expect("digest");
    let config = Config::new(
        ExpectedIdentity::UNVERIFIED,
        FirmwareImage::recorded("synthetic-test-image.bin", digest, 0x2_4000, "synthetic"),
        plan,
        Limits::DEFAULT,
        LAYOUT,
    )
    .expect("config");

    let mut machine = Machine::new(config);
    let mut action = machine.start().expect("start");
    let mut steps = 0u32;
    loop {
        steps = steps.saturating_add(1);
        assert!(steps < 32, "state machine did not settle");
        action = match action {
            Action::ReadIdentity => machine
                .on(Event::Identified(Identity {
                    vendor: 0x1DE4,
                    device: 0x0001,
                }))
                .expect("identity"),
            Action::AwaitBootRom => machine.on(Event::BootRomReady).expect("boot rom"),
            Action::SendChunk(chunk) => machine
                .on(Event::Ack {
                    sequence: chunk.index as u16,
                })
                .expect("ack"),
            Action::RetryChunk(_) => panic!("a fault-free run must not retry"),
            Action::RequestVerify => machine.on(Event::VerifyPassed).expect("verify"),
            Action::StartFirmware => break,
            Action::Fault(fault) => panic!("unexpected fault {fault:?}"),
        };
    }
    assert_eq!(machine.state(), State::Ready);
    assert_eq!(machine.transfer().next_index(), 9);
    assert_eq!(steps, 13);
}

#[test]
fn the_firmware_record_never_carries_the_blob() {
    let image = FirmwareImage::UNRECORDED;
    assert!(!image.is_recorded());
    assert!(image.digest.is_none());
    assert!(image.size.is_none());
    assert!(image.path.starts_with("UNKNOWN"));
}
