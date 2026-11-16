// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for interrupt status decoding.

use super::*;
use crate::regs::{IRQ_MDIO_DONE, IRQ_MDIO_ERROR, IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};

#[test]
fn an_empty_status_has_no_work_and_no_unknown_bits() {
    let status = InterruptStatus::from_raw(0);
    assert!(status.is_empty());
    assert!(!status.has_work());
    assert_eq!(status.unknown(), 0);
    assert_eq!(status.ack_bits(), 0);
}

#[test]
fn every_known_interrupt_decodes() {
    let status =
        InterruptStatus::from_raw(IRQ_RXDMA_DONE | IRQ_TXDMA_DONE | IRQ_MDIO_DONE | IRQ_MDIO_ERROR);
    assert!(status.rx_done());
    assert!(status.tx_done());
    assert!(status.mdio_done());
    assert!(status.mdio_error());
    assert!(status.has_work());
    assert_eq!(status.unknown(), 0);
    assert_eq!(status.ack_bits(), status.raw());
}

#[test]
fn rx_and_tx_done_are_independent() {
    let rx = InterruptStatus::from_raw(IRQ_RXDMA_DONE);
    assert!(rx.rx_done());
    assert!(!rx.tx_done());
    let tx = InterruptStatus::from_raw(IRQ_TXDMA_DONE);
    assert!(!tx.rx_done());
    assert!(tx.tx_done());
}

#[test]
fn mdio_done_and_error_are_independent() {
    let done = InterruptStatus::from_raw(IRQ_MDIO_DONE);
    assert!(done.mdio_done());
    assert!(!done.mdio_error());
    let error = InterruptStatus::from_raw(IRQ_MDIO_ERROR);
    assert!(!error.mdio_done());
    assert!(error.mdio_error());
    assert!(!error.has_work());
}

#[test]
fn unknown_bits_are_preserved() {
    let status = InterruptStatus::from_raw(IRQ_RXDMA_DONE | (1 << 30) | (1 << 2));
    assert_eq!(status.unknown(), (1 << 30) | (1 << 2));
    assert!(status.rx_done());
}

#[test]
fn ack_bits_round_trip_the_raw_word() {
    let raw = 0x8000_0000 | IRQ_TXDMA_DONE | IRQ_MDIO_ERROR;
    assert_eq!(InterruptStatus::from_raw(raw).ack_bits(), raw);
}
