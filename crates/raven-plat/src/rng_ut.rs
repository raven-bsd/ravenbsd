// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the RNG200 state machine and the fake source.

use alloc::vec::Vec;

use super::*;

struct TestRegisters {
    control: u32,
    status: u32,
    count: u32,
    data: [u32; 4],
    next: usize,
    writes: Vec<(usize, u32)>,
}

impl TestRegisters {
    fn new(count: u32) -> Self {
        Self {
            control: CONTROL_RBG_MASK,
            status: 0,
            count,
            data: [0x0403_0201, 0x0807_0605, 0x0c0b_0a09, 0],
            next: 0,
            writes: Vec::new(),
        }
    }
}

impl Registers for TestRegisters {
    fn read32(&mut self, offset: usize) -> Result<u32, Errno> {
        match offset {
            RNG_CONTROL => Ok(self.control),
            RNG_STATUS => Ok(self.status),
            RNG_COUNT => Ok(self.count),
            RNG_DATA => {
                let value = self.data.get(self.next).copied().unwrap_or(0);
                self.next = self.next.saturating_add(1);
                Ok(value)
            }
            RNG_RBG_RESET | RNG_RNG_RESET => Ok(0),
            _ => Err(Errno::EINVAL),
        }
    }

    fn write32(&mut self, offset: usize, value: u32) -> Result<(), Errno> {
        self.writes.push((offset, value));
        if offset == RNG_CONTROL {
            self.control = value;
        }
        Ok(())
    }
}

#[test]
fn init_clears_the_mask_and_enables_the_rbg() {
    let mut rng = Rng200::new(TestRegisters::new(0));
    rng.init().expect("init");
    let registers = rng.registers;
    assert_eq!(registers.control & CONTROL_RBG_ENABLE, CONTROL_RBG_ENABLE);
    assert_eq!(
        registers.control & (CONTROL_RBG_MASK & !CONTROL_RBG_ENABLE),
        0
    );
    assert!(
        registers
            .writes
            .iter()
            .any(|(offset, value)| *offset == RNG_STATUS && *value == u32::MAX)
    );
}

#[test]
fn fill_reads_the_fifo_in_little_endian_order() {
    let mut rng = Rng200::new(TestRegisters::new(2));
    let mut bytes = [0u8; 8];
    let written = rng.fill(&mut bytes).expect("fill");
    assert_eq!(written, 8);
    assert_eq!(bytes, [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn an_empty_fifo_returns_eagain() {
    let mut rng = Rng200::new(TestRegisters::new(0));
    let mut bytes = [0u8; 8];
    assert_eq!(rng.fill(&mut bytes), Err(Errno::EAGAIN));
}

#[test]
fn a_health_failure_returns_eio() {
    let mut registers = TestRegisters::new(1);
    registers.status = STATUS_MASTER_FAIL;
    let mut rng = Rng200::new(registers);
    let mut bytes = [0u8; 4];
    assert_eq!(rng.fill(&mut bytes), Err(Errno::EIO));
}

#[test]
fn a_request_larger_than_the_bound_is_rejected() {
    let mut rng = Rng200::new(TestRegisters::new(1));
    let mut bytes = [0u8; MAX_RNG_BYTES + 1];
    assert_eq!(rng.fill(&mut bytes), Err(Errno::E2BIG));
    assert_eq!(rng.fill(&mut []), Ok(0));
}

#[test]
fn the_fake_source_is_deterministic_and_bounded() {
    let mut first = FakeRng::new();
    let mut second = FakeRng::new();
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    assert_eq!(first.fill(&mut a), Ok(32));
    assert_eq!(second.fill(&mut b), Ok(32));
    assert_eq!(a, b);
    assert!(a.iter().any(|byte| *byte != 0));
    let mut too_much = [0u8; MAX_RNG_BYTES + 1];
    assert_eq!(first.fill(&mut too_much), Err(Errno::E2BIG));
}
