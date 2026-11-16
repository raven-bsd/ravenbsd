// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::VecDeque;

use super::*;
use crate::trb::Trb;

#[derive(Default)]
struct FakeHost {
    events: VecDeque<Trb>,
    doorbells: Vec<(u8, u8)>,
    irqs: usize,
    allocated: usize,
    freed: usize,
}

impl Host for FakeHost {
    fn allocate_dma(&mut self, len: usize, align: usize) -> Option<DmaBuffer> {
        assert!(align.is_power_of_two());
        self.allocated += 1;
        Some(DmaBuffer {
            cpu: 0x1000,
            bus: 0x2000,
            len,
        })
    }

    fn free_dma(&mut self, _buffer: DmaBuffer) {
        self.freed += 1;
    }

    fn doorbell(&mut self, slot: u8, target: u8) {
        self.doorbells.push((slot, target));
    }

    fn wait_event(&mut self, events: &mut EventRing, _timeout_ns: u64) -> Option<Trb> {
        let event = self.events.pop_front()?;
        events.push(event).ok()?;
        events.pop()
    }

    fn acknowledge_irq(&mut self) {
        self.irqs += 1;
    }
}

fn sequence() -> [Trb; 3] {
    [
        Trb::setup(0x1122_3344_5566_7788),
        Trb::data(0x2000, 18),
        Trb::status(0),
    ]
}

#[test]
fn submit_completes_a_setup_data_status_sequence() {
    let mut host = FakeHost::default();
    host.events
        .push_back(Trb::transfer_event(1, 1, CompletionCode::Success, 18));
    let mut seam = XhciSeam::new(host).expect("seam");
    let completion = seam
        .submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &sequence(),
            },
            1_000_000,
        )
        .expect("completion");
    assert_eq!(completion.slot, 1);
    assert_eq!(completion.endpoint, 1);
    assert_eq!(completion.code, CompletionCode::Success);
    assert_eq!(completion.length, 18);
    assert_eq!(seam.transfer_ring().pending(), 0);
    assert_eq!(seam.host().doorbells, [(1, 1)]);
    assert_eq!(seam.host().irqs, 1);
}

#[test]
fn submit_rejects_bad_endpoints_and_empty_transfers() {
    let mut seam = XhciSeam::new(FakeHost::default()).expect("seam");
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 0,
                trbs: &sequence()
            },
            0
        ),
        Err(SeamError::Endpoint(0))
    );
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 32,
                trbs: &sequence()
            },
            0
        ),
        Err(SeamError::Endpoint(32))
    );
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &[]
            },
            0
        ),
        Err(SeamError::EmptyTransfer)
    );
    assert!(seam.host().doorbells.is_empty());
}

#[test]
fn submit_times_out_without_an_event() {
    let mut seam = XhciSeam::new(FakeHost::default()).expect("seam");
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &sequence()
            },
            10
        ),
        Err(SeamError::Timeout)
    );
    assert_eq!(seam.transfer_ring().pending(), 3);
    assert_eq!(seam.host().doorbells, [(1, 1)]);
}

#[test]
fn submit_rejects_a_mismatched_event() {
    let mut host = FakeHost::default();
    host.events
        .push_back(Trb::transfer_event(2, 1, CompletionCode::Success, 0));
    let mut seam = XhciSeam::new(host).expect("seam");
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &sequence()
            },
            10
        ),
        Err(SeamError::UnexpectedEvent)
    );
}

#[test]
fn submit_checks_the_whole_sequence_fits_before_ringing() {
    let mut seam = XhciSeam::new(FakeHost::default()).expect("seam");
    let too_many = [Trb::normal(0, 1, true); 16];
    assert_eq!(
        seam.submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &too_many
            },
            10
        ),
        Err(SeamError::Ring(RingError::Full))
    );
    assert_eq!(seam.transfer_ring().pending(), 0);
}

#[test]
fn dma_and_command_ring_are_platform_hooks() {
    let mut seam = XhciSeam::new(FakeHost::default()).expect("seam");
    let buffer = seam.allocate(4096, 4096).expect("dma");
    assert_eq!(buffer.len, 4096);
    assert_eq!(seam.host().allocated, 1);
    seam.free(buffer);
    assert_eq!(seam.host().freed, 1);
    seam.command_ring_mut()
        .push(Trb::enable_slot())
        .expect("command");
    assert_eq!(seam.command_ring().pending(), 1);
    assert_eq!(
        seam.command_ring().slot(0).map(Trb::kind),
        Some(TrbType::EnableSlot)
    );
}

#[test]
fn unrelated_events_are_skipped_until_the_transfer_event() {
    let mut host = FakeHost::default();
    host.events.push_back(Trb::port_status_change(1));
    host.events
        .push_back(Trb::transfer_event(1, 1, CompletionCode::ShortPacket, 4));
    let mut seam = XhciSeam::new(host).expect("seam");
    let completion = seam
        .submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &sequence(),
            },
            10,
        )
        .expect("completion");
    assert_eq!(completion.code, CompletionCode::ShortPacket);
    assert_eq!(completion.length, 4);
}
