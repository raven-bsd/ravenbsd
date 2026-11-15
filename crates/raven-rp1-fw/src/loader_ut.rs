// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the firmware-load state machine.

use super::*;
use crate::sim::SimSeam;
use crate::test_fixture;

fn image(size: u64) -> raven_rp1::FirmwareImage {
    let digest = raven_rp1::Digest::from_bytes([0xAB; 32]);
    raven_rp1::FirmwareImage::recorded("synthetic:loader-test", digest, size, "synthetic")
}

fn ramp(len: usize) -> Vec<u8> {
    (0..len).map(|value| value as u8).collect()
}

#[test]
fn the_synthetic_fixture_loads_completely() {
    let blob = test_fixture::blob();
    let image = test_fixture::image();
    let plan = Plan::new(64, 16).expect("plan");
    let mut loader = Loader::new(image, &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");

    let outcome = loader.run(&mut seam).expect("run");
    assert_eq!(
        outcome,
        LoadOutcome {
            chunks: 4,
            steps: 16
        }
    );
    assert_eq!(loader.state(), State::Running);
    assert_eq!(loader.index(), 4);
    assert!(seam.started());
    assert!(seam.verify());
    assert_eq!(seam.written(), 64);
    assert_eq!(seam.doorbells(), 4);
    assert_eq!(seam.resets(), 1);
    assert_eq!(loader.step(&mut seam), Ok(Step::Done));
}

#[test]
fn one_step_per_seam_operation() {
    let blob = ramp(32);
    let plan = Plan::new(32, 16).expect("plan");
    let mut loader = Loader::new(image(32), &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");

    let mut steps = Vec::new();
    loop {
        let step = loader.step(&mut seam).expect("step");
        steps.push(step);
        if matches!(step, Step::Done) {
            break;
        }
    }
    let chunks: Vec<Chunk> = (0..plan.chunks())
        .filter_map(|index| plan.chunk(index))
        .collect();
    assert_eq!(
        steps,
        vec![
            Step::Reset,
            Step::PollBootRom,
            Step::WriteChunk(chunks[0]),
            Step::Doorbell(chunks[0]),
            Step::PollStatus,
            Step::WriteChunk(chunks[1]),
            Step::Doorbell(chunks[1]),
            Step::PollStatus,
            Step::Start,
            Step::Done,
        ]
    );
}

#[test]
fn a_short_last_chunk_is_written() {
    let blob = ramp(70);
    let plan = Plan::new(70, 16).expect("plan");
    let mut loader = Loader::new(image(70), &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    let outcome = loader.run(&mut seam).expect("run");
    assert_eq!(outcome.chunks, 5);
    assert!(seam.verify());
    assert_eq!(seam.written(), 70);
}

#[test]
fn a_missing_boot_rom_retries_then_faults() {
    let blob = ramp(16);
    let plan = Plan::new(16, 16).expect("plan");
    let limits = Limits {
        max_timeouts: 2,
        max_busy_polls: 2,
    };
    let mut loader = Loader::new(image(16), &blob, plan, limits).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_ready(false);

    assert_eq!(loader.step(&mut seam), Ok(Step::Reset));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert_eq!(loader.step(&mut seam), Err(FwError::Timeout));
    assert_eq!(loader.state(), State::Faulted);
    assert_eq!(loader.step(&mut seam), Err(FwError::Faulted));

    loader.reset();
    seam.set_ready(true);
    assert!(loader.run(&mut seam).is_ok());
    assert_eq!(loader.state(), State::Running);
}

#[test]
fn a_busy_device_is_bounded() {
    let blob = ramp(16);
    let plan = Plan::new(16, 16).expect("plan");
    let limits = Limits {
        max_timeouts: 2,
        max_busy_polls: 1,
    };
    let mut loader = Loader::new(image(16), &blob, plan, limits).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_busy_polls(2);

    assert_eq!(loader.step(&mut seam), Ok(Step::Reset));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert!(matches!(loader.step(&mut seam), Ok(Step::WriteChunk(_))));
    assert!(matches!(loader.step(&mut seam), Ok(Step::Doorbell(_))));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollStatus));
    assert_eq!(loader.step(&mut seam), Err(FwError::Busy));
    assert_eq!(loader.state(), State::Faulted);
}

#[test]
fn a_device_fault_stops_the_load() {
    let blob = ramp(16);
    let plan = Plan::new(16, 16).expect("plan");
    let mut loader = Loader::new(image(16), &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_fault(Some(0x42));

    assert_eq!(loader.step(&mut seam), Ok(Step::Reset));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert!(matches!(loader.step(&mut seam), Ok(Step::WriteChunk(_))));
    assert!(matches!(loader.step(&mut seam), Ok(Step::Doorbell(_))));
    assert_eq!(loader.step(&mut seam), Err(FwError::DeviceFault(0x42)));
    assert_eq!(loader.state(), State::Faulted);
}

#[test]
fn a_write_error_and_a_reset_error_are_reported() {
    let blob = ramp(16);
    let plan = Plan::new(16, 16).expect("plan");
    let mut loader = Loader::new(image(16), &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_fail_reset(true);
    assert_eq!(loader.step(&mut seam), Err(FwError::Seam(SeamError::Bus)));

    let mut loader = Loader::new(image(16), &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_fail_write(true);
    assert_eq!(loader.step(&mut seam), Ok(Step::Reset));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert_eq!(loader.step(&mut seam), Err(FwError::Seam(SeamError::Bus)));
}

#[test]
fn a_status_timeout_uses_the_budget() {
    let blob = ramp(16);
    let plan = Plan::new(16, 16).expect("plan");
    let limits = Limits {
        max_timeouts: 0,
        max_busy_polls: 0,
    };
    let mut loader = Loader::new(image(16), &blob, plan, limits).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    seam.set_status_error(Some(SeamError::Timeout));

    assert_eq!(loader.step(&mut seam), Ok(Step::Reset));
    assert_eq!(loader.step(&mut seam), Ok(Step::PollBootRom));
    assert!(matches!(loader.step(&mut seam), Ok(Step::WriteChunk(_))));
    assert!(matches!(loader.step(&mut seam), Ok(Step::Doorbell(_))));
    assert_eq!(loader.step(&mut seam), Err(FwError::Timeout));
}

#[test]
fn inconsistent_metadata_is_rejected() {
    let blob = ramp(32);
    let plan = Plan::new(32, 16).expect("plan");
    assert_eq!(
        Loader::new(
            raven_rp1::FirmwareImage::UNRECORDED,
            &blob,
            plan,
            Limits::DEFAULT
        ),
        Err(FwError::Unrecorded)
    );
    assert!(matches!(
        Loader::new(
            image(64),
            &blob,
            Plan::new(32, 16).expect("plan"),
            Limits::DEFAULT
        ),
        Err(FwError::BlobMismatch { .. })
    ));
    assert!(matches!(
        Loader::new(
            image(32),
            &blob,
            Plan::new(64, 16).expect("plan"),
            Limits::DEFAULT
        ),
        Err(FwError::PlanMismatch { .. })
    ));
}
