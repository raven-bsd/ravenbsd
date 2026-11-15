// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the enumeration and firmware-load state machine.

use super::*;
use crate::firmware::Digest;
use crate::frame::FrameError;
use crate::transfer::PlanError;

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

/// A synthetic digest; not an RP1 firmware hash.
const DIGEST: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn image(size: u64) -> FirmwareImage {
    let digest = Digest::from_hex(DIGEST).expect("digest");
    FirmwareImage::recorded("synthetic-test-image.bin", digest, size, "synthetic")
}

fn config_with(
    expected: ExpectedIdentity,
    firmware: FirmwareImage,
    image_len: u32,
    chunk_len: u32,
    limits: Limits,
) -> Config {
    let plan = TransferPlan::new(image_len, chunk_len).expect("plan");
    Config::new(expected, firmware, plan, limits, LAYOUT).expect("config")
}

fn config(image_len: u32, chunk_len: u32) -> Config {
    config_with(
        ExpectedIdentity::UNVERIFIED,
        image(u64::from(image_len)),
        image_len,
        chunk_len,
        Limits::DEFAULT,
    )
}

fn identified(machine: &mut Machine) -> Action {
    machine.start().expect("start");
    machine
        .on(Event::Identified(Identity {
            vendor: 0x1DE4,
            device: 0x0001,
        }))
        .expect("identity")
}

fn loading(machine: &mut Machine) -> Action {
    identified(machine);
    machine.on(Event::BootRomReady).expect("boot rom")
}

#[test]
fn the_happy_path_loads_every_chunk_and_starts() {
    let mut machine = Machine::new(config(0x2_8000, 0x1_0000));
    assert_eq!(machine.state(), State::Idle);
    assert_eq!(machine.start(), Ok(Action::ReadIdentity));
    assert_eq!(machine.state(), State::Identifying);

    assert_eq!(
        machine.on(Event::Identified(Identity {
            vendor: 0x1DE4,
            device: 0x0001
        })),
        Ok(Action::AwaitBootRom)
    );
    assert_eq!(machine.state(), State::Enumerated);

    assert_eq!(
        machine.on(Event::BootRomReady),
        Ok(Action::SendChunk(Chunk {
            index: 0,
            offset: 0,
            length: 0x1_0000
        }))
    );
    assert_eq!(machine.state(), State::Loading);

    assert_eq!(
        machine.on(Event::Ack { sequence: 0 }),
        Ok(Action::SendChunk(Chunk {
            index: 1,
            offset: 0x1_0000,
            length: 0x1_0000
        }))
    );
    assert_eq!(
        machine.on(Event::Ack { sequence: 1 }),
        Ok(Action::SendChunk(Chunk {
            index: 2,
            offset: 0x2_0000,
            length: 0x8000
        }))
    );
    assert_eq!(
        machine.on(Event::Ack { sequence: 2 }),
        Ok(Action::RequestVerify)
    );
    assert_eq!(machine.state(), State::Verifying);
    assert_eq!(machine.on(Event::VerifyPassed), Ok(Action::StartFirmware));
    assert_eq!(machine.state(), State::Ready);
    assert_eq!(machine.fault(), None);
}

#[test]
fn an_unrecorded_firmware_refuses_to_start() {
    let config = config_with(
        ExpectedIdentity::UNVERIFIED,
        FirmwareImage::UNRECORDED,
        0x1_0000,
        0x1_0000,
        Limits::DEFAULT,
    );
    let mut machine = Machine::new(config);
    assert_eq!(
        machine.start(),
        Ok(Action::Fault(Fault::FirmwareUnrecorded))
    );
    assert_eq!(machine.state(), State::Faulted);
}

#[test]
fn an_identity_mismatch_is_a_fault() {
    let config = config_with(
        ExpectedIdentity::exact(0x1DE4, 0x0001),
        image(0x1_0000),
        0x1_0000,
        0x1_0000,
        Limits::DEFAULT,
    );
    let mut machine = Machine::new(config);
    assert_eq!(machine.start(), Ok(Action::ReadIdentity));
    let action = machine
        .on(Event::Identified(Identity {
            vendor: 0x1234,
            device: 0x5678,
        }))
        .expect("identity");
    assert!(matches!(
        action,
        Action::Fault(Fault::IdentityMismatch {
            observed: Identity {
                vendor: 0x1234,
                device: 0x5678
            },
            ..
        })
    ));
    assert_eq!(machine.state(), State::Faulted);
}

#[test]
fn an_out_of_order_ack_is_a_fault() {
    let mut machine = Machine::new(config(0x2_0000, 0x1_0000));
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));
    let action = machine.on(Event::Ack { sequence: 1 }).expect("ack");
    assert_eq!(
        action,
        Action::Fault(Fault::UnexpectedAck {
            expected: 0,
            observed: 1
        })
    );
    assert_eq!(machine.state(), State::Faulted);
}

#[test]
fn a_chunk_rejection_retries_then_faults() {
    let limits = Limits {
        max_retries: 1,
        max_timeouts: 2,
    };
    let config = config_with(
        ExpectedIdentity::UNVERIFIED,
        image(0x2_0000),
        0x2_0000,
        0x1_0000,
        limits,
    );
    let mut machine = Machine::new(config);
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));

    assert_eq!(
        machine.on(Event::Reject { sequence: 0 }),
        Ok(Action::RetryChunk(Chunk {
            index: 0,
            offset: 0,
            length: 0x1_0000
        }))
    );
    let action = machine.on(Event::Reject { sequence: 0 }).expect("reject");
    assert_eq!(
        action,
        Action::Fault(Fault::ChunkRejected {
            index: 0,
            attempts: 2
        })
    );
    assert_eq!(machine.state(), State::Faulted);
}

#[test]
fn timeouts_are_bounded_per_step() {
    let limits = Limits {
        max_retries: 3,
        max_timeouts: 1,
    };
    let config = config_with(
        ExpectedIdentity::UNVERIFIED,
        image(0x2_0000),
        0x2_0000,
        0x1_0000,
        limits,
    );
    let mut machine = Machine::new(config);
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));

    assert_eq!(
        machine.on(Event::Timeout),
        Ok(Action::RetryChunk(Chunk {
            index: 0,
            offset: 0,
            length: 0x1_0000
        }))
    );
    let action = machine.on(Event::Timeout).expect("timeout");
    assert_eq!(
        action,
        Action::Fault(Fault::Timeout {
            state: State::Loading
        })
    );
    assert_eq!(machine.state(), State::Faulted);
}

#[test]
fn a_successful_step_resets_the_timeout_budget() {
    let limits = Limits {
        max_retries: 3,
        max_timeouts: 1,
    };
    let config = config_with(
        ExpectedIdentity::UNVERIFIED,
        image(0x2_0000),
        0x2_0000,
        0x1_0000,
        limits,
    );
    let mut machine = Machine::new(config);
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));
    assert!(matches!(
        machine.on(Event::Timeout).expect("timeout"),
        Action::RetryChunk(_)
    ));
    assert!(matches!(
        machine.on(Event::Ack { sequence: 0 }).expect("ack"),
        Action::SendChunk(_)
    ));
    assert!(matches!(
        machine.on(Event::Timeout).expect("timeout"),
        Action::RetryChunk(_)
    ));
}

#[test]
fn a_malformed_response_is_terminal_until_reset() {
    let mut machine = Machine::new(config(0x1_0000, 0x1_0000));
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));
    assert_eq!(
        machine.on(Event::Malformed),
        Ok(Action::Fault(Fault::MalformedResponse))
    );
    assert_eq!(
        machine.on(Event::Ack { sequence: 0 }),
        Err(MachineError::UnexpectedEvent {
            state: State::Faulted,
            event: Event::Ack { sequence: 0 }
        })
    );
    machine.reset();
    assert_eq!(machine.state(), State::Idle);
    assert_eq!(machine.fault(), None);
    assert_eq!(machine.start(), Ok(Action::ReadIdentity));
}

#[test]
fn a_verify_failure_is_a_fault() {
    let mut machine = Machine::new(config(0x1_0000, 0x1_0000));
    assert!(matches!(loading(&mut machine), Action::SendChunk(_)));
    assert_eq!(
        machine.on(Event::Ack { sequence: 0 }),
        Ok(Action::RequestVerify)
    );
    assert_eq!(
        machine.on(Event::VerifyFailed),
        Ok(Action::Fault(Fault::VerifyFailed))
    );
}

#[test]
fn events_outside_their_state_leave_it_untouched() {
    let mut machine = Machine::new(config(0x1_0000, 0x1_0000));
    assert_eq!(
        machine.on(Event::BootRomReady),
        Err(MachineError::UnexpectedEvent {
            state: State::Idle,
            event: Event::BootRomReady
        })
    );
    assert_eq!(machine.state(), State::Idle);
    assert_eq!(machine.start(), Ok(Action::ReadIdentity));
    assert_eq!(
        machine.start(),
        Err(MachineError::AlreadyStarted {
            state: State::Identifying
        })
    );
}

#[test]
fn the_config_rejects_inconsistent_data() {
    let plan = TransferPlan::new(0x1_0000, 0x1000).expect("plan");
    assert_eq!(
        Config::new(
            ExpectedIdentity::UNVERIFIED,
            image(0x8000),
            plan,
            Limits::DEFAULT,
            LAYOUT
        ),
        Err(ConfigError::FirmwareSizeMismatch {
            firmware: 0x8000,
            plan: 0x1_0000
        })
    );
    let mut broken = LAYOUT;
    broken.magic_mask = 0;
    assert_eq!(
        Config::new(
            ExpectedIdentity::UNVERIFIED,
            image(0x1_0000),
            plan,
            Limits::DEFAULT,
            broken
        ),
        Err(ConfigError::Frame(FrameError::Layout))
    );
}

#[test]
fn a_bad_plan_is_rejected_before_the_machine_exists() {
    assert_eq!(TransferPlan::new(0, 0x1000), Err(PlanError::EmptyImage));
}
