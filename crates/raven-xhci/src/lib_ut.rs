// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface assertions for the xHCI crate.

use super::*;

#[test]
fn the_dma_layout_fits_the_smallest_buffer() {
    const {
        assert!(INTERRUPT_BUFFER_OFFSET + 0x100 <= MIN_DMA_LEN);
    }
    assert_eq!(EP0_DCI, 1);
    assert_eq!(EP1_DCI, 3);
    assert_eq!(RING_TRBS, 64);
    assert_eq!(context_size(true), 64);
    assert_eq!(input_context_size(64), MAX_INPUT_CONTEXT);
}

#[test]
fn the_error_type_reports_completion_codes() {
    assert_eq!(
        XhciError::Completion(6).to_string(),
        "xHCI completion code 6"
    );
    assert_eq!(XhciError::Timeout.to_string(), "xHCI operation timed out");
}
