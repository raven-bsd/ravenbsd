// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the chunk plan.

use super::*;

#[test]
fn invalid_plans_are_rejected() {
    assert_eq!(Plan::new(0, 16), Err(PlanError::EmptyBlob));
    assert_eq!(Plan::new(1, 0), Err(PlanError::EmptyChunk));
    assert_eq!(
        Plan::new(1, MAX_CHUNK + 1),
        Err(PlanError::ChunkTooLarge {
            length: MAX_CHUNK + 1
        })
    );
    assert_eq!(
        Plan::new(u32::MAX, 1),
        Err(PlanError::TooManyChunks { chunks: u32::MAX })
    );
}

#[test]
fn an_exact_multiple_has_no_short_chunk() {
    let plan = Plan::new(64, 16).expect("plan");
    assert_eq!(plan.blob_len(), 64);
    assert_eq!(plan.chunk_len(), 16);
    assert_eq!(plan.chunks(), 4);
    assert_eq!(
        plan.chunk(3),
        Some(Chunk {
            index: 3,
            offset: 48,
            length: 16
        })
    );
    assert_eq!(plan.chunk(4), None);
}

#[test]
fn a_remainder_becomes_a_short_last_chunk() {
    let plan = Plan::new(70, 16).expect("plan");
    assert_eq!(plan.chunks(), 5);
    assert_eq!(
        plan.chunk(4),
        Some(Chunk {
            index: 4,
            offset: 64,
            length: 6
        })
    );
}

#[test]
fn the_largest_chunk_is_accepted() {
    let plan = Plan::new(8 * 1024, MAX_CHUNK).expect("plan");
    assert_eq!(plan.chunks(), 2);
    assert_eq!(plan.chunk(1).map(|chunk| chunk.length), Some(MAX_CHUNK));
}
