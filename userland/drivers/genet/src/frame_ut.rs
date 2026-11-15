// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the userspace GENET frame path and link updates.

use super::*;
use crate::tests::{FakeSeam, controller, ready_controller};
use raven_genet::frame::{FrameError, SlotOwner};
use raven_genet::link::LinkEvent;
use raven_genet::phy::{AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK, Duplex, LinkError, Speed};
use raven_genet::ring::{
    RX_STATUS_BUFLEN_SHIFT, RX_STATUS_EOP, RX_STATUS_SOP, RxStatus, TX_STATUS_OWN,
};

fn backend() -> FrameBackend<FakeSeam> {
    FrameBackend::new(ready_controller()).expect("backend")
}

fn rx_frame(length: u32) -> RxStatus {
    RxStatus::from_raw((length << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP)
}

fn up_1000() -> (u16, u16) {
    (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
}

#[test]
fn a_frame_backend_requires_the_ready_stage() {
    assert_eq!(
        FrameBackend::new(controller()),
        Err(DriverError::WrongStage {
            expected: Stage::Ready,
            actual: Stage::Attached,
        })
    );
}

#[test]
fn posting_rx_uses_the_first_seam_buffer() {
    let mut backend = backend();
    let post = backend.post_rx_next().expect("post");
    assert_eq!(post.index, 0);
    assert_eq!(post.descriptor.address, 0x1000);
    assert_eq!(backend.path().rx.slot_owner(0), Some(SlotOwner::Device));
    assert_eq!(backend.path().rx.posted(), Ok(1));
}

#[test]
fn rx_refill_reuses_the_next_slot() {
    let mut backend = backend();
    backend.post_rx_next().expect("post");
    backend
        .complete_rx(rx_frame(64))
        .expect("complete")
        .expect("entry");
    let refill = backend.refill_rx_next().expect("refill");
    assert_eq!(refill.index, 1);
    assert_eq!(refill.descriptor.address, 0x1800);
}

#[test]
fn a_ninth_rx_post_reports_the_missing_seam_buffer() {
    let mut backend = backend();
    for _ in 0..8 {
        backend.post_rx_next().expect("post");
    }
    assert_eq!(backend.post_rx_next(), Err(DriverError::MissingBuffer(8)));
}

#[test]
fn completing_a_frame_delivers_its_payload_length() {
    let mut backend = backend();
    backend.post_rx_next().expect("post");
    let completion = backend
        .complete_rx(rx_frame(64))
        .expect("complete")
        .expect("entry");
    assert_eq!(completion.frame.expect("frame").length, 62);
    assert_eq!(backend.path().rx.posted(), Ok(0));
    assert_eq!(backend.path().rx.slot_owner(0), Some(SlotOwner::Free));
}

#[test]
fn queueing_tx_uses_the_first_seam_buffer() {
    let mut backend = backend();
    let post = backend.queue_tx_next(64).expect("queue");
    assert_eq!(post.index, 0);
    assert_eq!(post.descriptor.address, 0x8000);
    assert_eq!(post.descriptor.status.length(), 64);
    assert_eq!(backend.path().tx.posted(), Ok(1));
}

#[test]
fn a_tx_frame_is_reclaimed_after_completion() {
    let mut backend = backend();
    let post = backend.queue_tx_next(64).expect("queue");
    let completion = backend
        .reclaim_tx(post.descriptor.status)
        .expect("reclaim")
        .expect("entry");
    assert_eq!(completion.index, 0);
    assert_eq!(backend.path().tx.posted(), Ok(0));
}

#[test]
fn a_still_owned_tx_descriptor_is_not_reclaimed() {
    let mut backend = backend();
    let post = backend.queue_tx_next(64).expect("queue");
    let owned = TxStatus::from_raw(post.descriptor.status.raw() | TX_STATUS_OWN);
    assert_eq!(
        backend.reclaim_tx(owned),
        Err(DriverError::Frame(FrameError::StillOwned(0)))
    );
}

#[test]
fn a_tx_frame_longer_than_the_seam_buffer_is_rejected() {
    let mut backend = backend();
    assert_eq!(
        backend.queue_tx_next(BUFFER_LEN + 1),
        Err(DriverError::ShortBuffer {
            index: 0,
            len: BUFFER_LEN,
        })
    );
}

#[test]
fn link_up_sets_the_umac_speed_and_rgmii_link() {
    let mut backend = FrameBackend::with_debounce(ready_controller(), 2).expect("backend");
    let (bmsr, auxsts) = up_1000();
    assert_eq!(backend.observe_link(bmsr, auxsts), Ok(None));
    assert_eq!(
        backend.observe_link(bmsr, auxsts),
        Ok(Some(LinkEvent::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }))
    );
    assert_eq!(
        backend.controller().seam.register(regs::UMAC_CMD) & regs::UMAC_CMD_SPEED_MASK,
        regs::UMAC_CMD_SPEED_1000 << regs::UMAC_CMD_SPEED_SHIFT
    );
    assert_eq!(
        backend.controller().seam.register(regs::EXT_RGMII_OOB_CTRL)
            & regs::EXT_RGMII_OOB_RGMII_LINK,
        regs::EXT_RGMII_OOB_RGMII_LINK
    );
    assert_eq!(
        backend.link_state(),
        LinkState::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }
    );
}

#[test]
fn link_down_clears_the_rgmii_link_bit() {
    let mut backend = FrameBackend::with_debounce(ready_controller(), 1).expect("backend");
    let (bmsr, auxsts) = up_1000();
    backend.observe_link(bmsr, auxsts).expect("up");
    assert_eq!(backend.observe_link(0, 0), Ok(Some(LinkEvent::Down)));
    assert_eq!(
        backend.controller().seam.register(regs::EXT_RGMII_OOB_CTRL)
            & regs::EXT_RGMII_OOB_RGMII_LINK,
        0
    );
    assert_eq!(backend.link_state(), LinkState::Down);
}

#[test]
fn an_unknown_link_resolution_is_reported_without_a_register_change() {
    let mut backend = FrameBackend::with_debounce(ready_controller(), 1).expect("backend");
    let before = backend.controller().seam.register(regs::EXT_RGMII_OOB_CTRL);
    assert_eq!(
        backend.observe_link(BMSR_LINK, 0),
        Err(DriverError::Link(LinkError::UnknownResolution(0)))
    );
    assert_eq!(
        backend.controller().seam.register(regs::EXT_RGMII_OOB_CTRL),
        before
    );
    assert_eq!(backend.link_state(), LinkState::Unknown);
}

#[test]
fn a_debounce_of_one_applies_the_first_sample() {
    let mut backend = FrameBackend::with_debounce(ready_controller(), 1).expect("backend");
    let (bmsr, auxsts) = up_1000();
    assert_eq!(
        backend.observe_link(bmsr, auxsts),
        Ok(Some(LinkEvent::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }))
    );
}

#[test]
fn the_frame_path_has_the_full_descriptor_count() {
    let backend = backend();
    assert_eq!(backend.path().rx.count(), regs::DESC_COUNT);
    assert_eq!(backend.path().tx.count(), regs::DESC_COUNT);
    assert_eq!(
        backend.path().rx.free_slots(),
        Ok(regs::DESC_COUNT as usize)
    );
    assert_eq!(
        backend.path().tx.free_slots(),
        Ok(regs::DESC_COUNT as usize)
    );
}
