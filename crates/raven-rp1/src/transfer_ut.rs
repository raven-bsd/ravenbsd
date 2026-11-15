// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for chunk planning and transfer transitions.

use super::*;

#[test]
fn an_exact_multiple_has_no_short_chunk() {
    let plan = TransferPlan::new(0x40_0000, 0x1_0000).expect("plan");
    assert_eq!(plan.chunks(), 64);
    assert_eq!(
        plan.chunk(63),
        Some(Chunk {
            index: 63,
            offset: 0x3f_0000,
            length: 0x1_0000
        })
    );
    assert_eq!(plan.chunk(64), None);
}

#[test]
fn a_remainder_becomes_a_short_last_chunk() {
    let plan = TransferPlan::new(0x2_0001, 0x1_0000).expect("plan");
    assert_eq!(plan.chunks(), 3);
    assert_eq!(
        plan.chunk(2),
        Some(Chunk {
            index: 2,
            offset: 0x2_0000,
            length: 1
        })
    );
}

#[test]
fn invalid_plans_are_rejected() {
    assert_eq!(TransferPlan::new(0, 0x1000), Err(PlanError::EmptyImage));
    assert_eq!(TransferPlan::new(1, 0), Err(PlanError::EmptyChunk));
    assert_eq!(
        TransferPlan::new(1, MAX_CHUNK_LEN + 1),
        Err(PlanError::ChunkTooLarge {
            length: MAX_CHUNK_LEN + 1
        })
    );
    assert_eq!(
        TransferPlan::new(u32::MAX, 0x1000),
        Err(PlanError::TooManyChunks { chunks: 1_048_576 })
    );
}

#[test]
fn an_ack_advances_and_completes() {
    let plan = TransferPlan::new(0x1_8000, 0x1_0000).expect("plan");
    let mut transfer = Transfer::new(plan);
    assert_eq!(transfer.current().map(|chunk| chunk.index), Some(0));
    assert_eq!(
        transfer.acked(0).expect("ack").map(|chunk| chunk.index),
        Some(1)
    );
    assert_eq!(transfer.acked(1).expect("ack"), None);
    assert!(transfer.is_complete());
    assert_eq!(transfer.acked(2), Err(TransferError::Complete));
}

#[test]
fn an_out_of_order_ack_is_rejected() {
    let plan = TransferPlan::new(0x2_0000, 0x1_0000).expect("plan");
    let mut transfer = Transfer::new(plan);
    assert_eq!(
        transfer.acked(1),
        Err(TransferError::OutOfOrder {
            expected: 0,
            got: 1
        })
    );
    assert_eq!(transfer.next_index(), 0);
}

#[test]
fn retries_are_bounded() {
    let plan = TransferPlan::new(0x2_0000, 0x1_0000).expect("plan");
    let mut transfer = Transfer::new(plan);
    assert_eq!(transfer.retry(0, 2).map(|chunk| chunk.index), Ok(0));
    assert_eq!(transfer.attempts(), 1);
    assert_eq!(transfer.retry(0, 2).map(|chunk| chunk.index), Ok(0));
    assert_eq!(transfer.attempts(), 2);
    assert_eq!(
        transfer.retry(0, 2),
        Err(TransferError::RetriesExhausted {
            index: 0,
            attempts: 3
        })
    );
}

#[test]
fn a_successful_ack_clears_the_retry_count() {
    let plan = TransferPlan::new(0x2_0000, 0x1_0000).expect("plan");
    let mut transfer = Transfer::new(plan);
    assert_eq!(transfer.retry(0, 2).map(|chunk| chunk.index), Ok(0));
    assert_eq!(transfer.attempts(), 1);
    assert_eq!(
        transfer.acked(0).expect("ack").map(|chunk| chunk.index),
        Some(1)
    );
    assert_eq!(transfer.attempts(), 0);
}
