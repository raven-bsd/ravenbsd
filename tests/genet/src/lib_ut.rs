// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic stress and fault-injection loops for the GENET model.

use raven_fdt_lite::DmaWindow;
use raven_genet::dma::{DmaError, DmaMapping, DmaRules, MAX_DMA_BUFFER};
use raven_genet::dtb::DmaMap;
use raven_genet::frame::{FrameError, FramePath, ReclaimBudget, RxPath, TxPath};
use raven_genet::irq::InterruptStatus;
use raven_genet::link::{LinkState, LinkTracker};
use raven_genet::phy::{AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK};
use raven_genet::recovery::{ErrorRecovery, RecoveryAction};
use raven_genet::regs::{IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};
use raven_genet::ring::{
    RX_STATUS_BUFLEN_SHIFT, RX_STATUS_CRC_ERR, RX_STATUS_EOP, RX_STATUS_OWN, RX_STATUS_SOP,
    RxStatus, TX_STATUS_OWN, TxStatus,
};

fn rx_frame(length: u32) -> RxStatus {
    RxStatus::from_raw((length << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP)
}

fn completed_tx(status: TxStatus) -> TxStatus {
    TxStatus::from_raw(status.raw() & !TX_STATUS_OWN)
}

fn dma_rules() -> DmaRules {
    DmaRules::new(DmaMap::from_windows(&[
        DmaWindow {
            bus_base: 0x4_7c00_0000,
            cpu_base: 0xfc00_0000,
            size: 0x0380_0000,
        },
        DmaWindow {
            bus_base: 0,
            cpu_base: 0,
            size: 0x4_0000_0000,
        },
    ]))
}

#[test]
fn full_rx_ring_wraps_256_slots_repeatedly() {
    let mut path = RxPath::new(256).expect("rx path");
    for round in 0..4 {
        for index in 0..256 {
            let post = path.post_next(0x1000 + index as u64 * 0x800).expect("post");
            assert_eq!(post.index as usize, index, "round {round}");
        }
        assert_eq!(path.posted(), Ok(256));
        for index in 0..256 {
            let completion = path
                .complete_next(rx_frame(64))
                .expect("complete")
                .expect("entry");
            assert_eq!(completion.index as usize, index);
            assert!(completion.frame.is_ok());
        }
        assert_eq!(path.posted(), Ok(0));
    }
}

#[test]
fn full_tx_ring_wraps_256_slots_repeatedly() {
    let mut path = TxPath::new(256).expect("tx path");
    for round in 0..4 {
        for index in 0..256 {
            let post = path
                .queue_next(0x10_0000 + index as u64 * 0x800, 64)
                .expect("queue");
            assert_eq!(post.index as usize, index, "round {round}");
        }
        assert_eq!(path.posted(), Ok(256));
        for index in 0..256 {
            let status = completed_tx(path.descriptor(index as u16).expect("descriptor").status);
            let completion = path.reclaim_next(status).expect("reclaim").expect("entry");
            assert_eq!(completion.index as usize, index);
        }
        assert_eq!(path.posted(), Ok(0));
    }
}

#[test]
fn still_owned_descriptors_do_not_advance_and_recover() {
    let mut rx = RxPath::new(4).expect("rx path");
    rx.post_next(0x1000).expect("post");
    let owned_rx = RxStatus::from_raw(rx_frame(64).raw() | RX_STATUS_OWN);
    assert_eq!(rx.complete_next(owned_rx), Err(FrameError::StillOwned(0)));
    assert_eq!(rx.posted(), Ok(1));
    assert!(rx.complete_next(rx_frame(64)).expect("complete").is_some());
    assert_eq!(rx.posted(), Ok(0));

    let mut tx = TxPath::new(4).expect("tx path");
    let post = tx.queue_next(0x10_0000, 64).expect("queue");
    assert_eq!(
        tx.reclaim_next(post.descriptor.status),
        Err(FrameError::StillOwned(0))
    );
    assert_eq!(tx.posted(), Ok(1));
    let done = completed_tx(post.descriptor.status);
    assert!(tx.reclaim_next(done).expect("reclaim").is_some());
    assert_eq!(tx.posted(), Ok(0));
}

#[test]
fn malformed_rx_statuses_are_counted_without_stalling() {
    let mut path = RxPath::new(256).expect("rx path");
    for index in 0..256 {
        path.post_next(0x1000 + index as u64 * 0x800).expect("post");
    }
    let mut errors = 0usize;
    for index in 0..256 {
        let status = match index % 4 {
            0 => rx_frame(64),
            1 => RxStatus::from_raw(rx_frame(64).raw() | RX_STATUS_CRC_ERR),
            2 => rx_frame(1),
            _ => rx_frame(256),
        };
        let completion = path
            .complete_next(status)
            .expect("complete")
            .expect("entry");
        if completion.frame.is_err() {
            errors += 1;
        }
    }
    assert_eq!(errors, 128);
    assert_eq!(path.posted(), Ok(0));
}

#[test]
fn malformed_tx_completions_free_the_ring() {
    let mut path = TxPath::new(256).expect("tx path");
    for index in 0..256 {
        path.queue_next(0x10_0000 + index as u64 * 0x800, 64)
            .expect("queue");
    }
    let mut errors = 0usize;
    for index in 0..256 {
        let status = if index % 2 == 0 {
            TxStatus::from_raw(0)
        } else {
            completed_tx(path.descriptor(index as u16).expect("descriptor").status)
        };
        match path.reclaim_next(status) {
            Err(FrameError::BadStatus(_)) => errors += 1,
            Ok(Some(_)) => {}
            other => panic!("unexpected reclaim result: {other:?}"),
        }
    }
    assert_eq!(errors, 128);
    assert_eq!(path.posted(), Ok(0));
}

#[test]
fn bounce_boundaries_hold_over_long_address_sweeps() {
    let rules = dma_rules();
    let boundary = 0x4_0000_0000u64;
    let mut direct = 0usize;
    let mut bounced = 0usize;
    for index in 0..1024u64 {
        let cpu = 0x3_ffff_f000 + index * 64;
        let result = rules.route(cpu, 2048, None);
        if cpu + 2048 <= boundary {
            assert!(result.is_ok(), "direct address {cpu:#x}");
            direct += 1;
        } else {
            assert_eq!(result, Err(DmaError::BounceRequired), "edge {cpu:#x}");
            bounced += 1;
        }
    }
    assert!(direct > 0);
    assert!(bounced > 0);

    let crossing = 0x3_ffff_f000 + 33 * 64;
    let bounce = DmaMapping {
        cpu_addr: 0x20_0000,
        bus_addr: 0x20_0000,
        len: 2048,
        bounce: false,
    };
    assert!(
        rules
            .route(crossing, 2048, Some(bounce))
            .expect("bounce")
            .bounce
    );
    for index in 0..1024u64 {
        assert!(
            rules.route(0x20_0000 + index * 64, 2048, None).is_ok(),
            "low direct address {index}"
        );
    }
    assert_eq!(
        rules.route(0x1000, MAX_DMA_BUFFER + 1, None),
        Err(DmaError::TooLong(MAX_DMA_BUFFER + 1))
    );
}

#[test]
fn link_flaps_under_continuous_traffic() {
    let mut rx = RxPath::new(8).expect("rx path");
    let mut tx = TxPath::new(8).expect("tx path");
    let mut link = LinkTracker::new(2);
    let mut events = 0usize;

    for round in 0..64 {
        rx.post_next(0x1000 + (round % 8) as u64 * 0x800)
            .expect("rx post");
        let tx_post = tx
            .queue_next(0x10_0000 + (round % 8) as u64 * 0x800, 64)
            .expect("tx queue");

        let (bmsr, auxsts) = if round % 2 == 0 {
            (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
        } else {
            (0, 0)
        };
        link.observe(bmsr, auxsts).expect("link sample");
        if link.observe(bmsr, auxsts).expect("link sample").is_some() {
            events += 1;
        }

        rx.complete_next(rx_frame(64))
            .expect("rx complete")
            .expect("rx entry");
        let status = completed_tx(tx.descriptor(tx_post.index).expect("tx descriptor").status);
        tx.reclaim_next(status)
            .expect("tx reclaim")
            .expect("tx entry");
    }

    assert!(events >= 2);
    assert!(matches!(
        link.state(),
        LinkState::Up { .. } | LinkState::Down
    ));
    assert_eq!(rx.posted(), Ok(0));
    assert_eq!(tx.posted(), Ok(0));
}

#[test]
fn irq_coalescing_reclaims_every_descriptor() {
    let mut path = FramePath::new(256).expect("frame path");
    for index in 0..256 {
        path.rx
            .post_next(0x1000 + index as u64 * 0x800)
            .expect("rx post");
        path.tx
            .queue_next(0x10_0000 + index as u64 * 0x800, 64)
            .expect("tx queue");
    }
    let rx_statuses = [rx_frame(64); 256];
    let mut tx_statuses = [TxStatus::from_raw(0); 256];
    for (index, status) in tx_statuses.iter_mut().enumerate() {
        *status = completed_tx(
            path.tx
                .descriptor(index as u16)
                .expect("tx descriptor")
                .status,
        );
    }

    let first = path
        .reclaim(
            ReclaimBudget::new(1, 1),
            &rx_statuses[..4],
            &tx_statuses[..4],
        )
        .expect("first reclaim");
    assert_eq!(first.rx_completed, 1);
    assert_eq!(first.tx_completed, 1);
    assert!(first.rx_truncated);
    assert!(first.tx_truncated);

    let mut completed = 1usize;
    while path.rx.posted().expect("posted") > 0 {
        let report = path
            .reclaim(
                ReclaimBudget::new(7, 7),
                &rx_statuses[completed..],
                &tx_statuses[completed..],
            )
            .expect("reclaim");
        assert_eq!(report.rx_completed, report.tx_completed);
        assert!(report.rx_completed > 0);
        completed += report.rx_completed;
    }
    assert_eq!(completed, 256);
    assert_eq!(path.tx.posted(), Ok(0));
}

#[test]
fn interrupt_status_coalesces_rx_and_tx_causes() {
    let status = InterruptStatus::from_raw(IRQ_RXDMA_DONE | IRQ_TXDMA_DONE);
    assert!(status.rx_done());
    assert!(status.tx_done());
    assert!(status.has_work());
    assert_eq!(status.unknown(), 0);
    assert_eq!(status.ack_bits(), status.raw());
}

#[test]
fn recovery_budget_is_bounded_under_repeated_faults() {
    let mut recovery = ErrorRecovery::default();
    let mut resets = 0usize;
    for _ in 0..3 {
        for _ in 0..8 {
            if recovery.record_error() == RecoveryAction::ResetRings {
                resets += 1;
            }
        }
    }
    assert_eq!(resets, 3);
    assert_eq!(recovery.resets(), 3);
    for _ in 0..7 {
        assert_eq!(recovery.record_error(), RecoveryAction::None);
    }
    assert_eq!(recovery.record_error(), RecoveryAction::Fault);
    let mut recovery = ErrorRecovery::new(3, 1);
    recovery.record_error();
    recovery.record_error();
    recovery.record_success();
    assert_eq!(recovery.consecutive(), 0);
    assert_eq!(recovery.record_error(), RecoveryAction::None);
}
