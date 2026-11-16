// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the driver launch/handle-slot matrix.

use super::*;
use std::vec::Vec;

const BASE: [&str; 5] = ["bar=0", "bar_size=65536", "dma=1048576", "irq=189", "serve"];

fn with(extra: &[&'static str]) -> Vec<&'static str> {
    let mut args = BASE.to_vec();
    args.extend_from_slice(extra);
    args
}

#[test]
fn the_default_is_the_pci_layout() {
    let options = Options::parse(BASE).expect("pci options");
    assert_eq!(options.layout, GrantLayout::Pci);
    assert_eq!(options.bar_handle, 3);
    assert_eq!(options.dma_handle, 5);
    assert_eq!(options.irq_handle, 6);
    assert_eq!(options.bar, 0);
    assert_eq!(options.bar_size, 65536);
    assert_eq!(options.dma_size, 1048576);
    assert_eq!(options.irq, 189);
    assert!(!options.test);
}

#[test]
fn the_platform_token_selects_bar_dma_irq_slots() {
    for token in ["platform", "platform-slots", "slots=platform"] {
        let options = Options::parse(with(&[token])).expect("platform options");
        assert_eq!(options.layout, GrantLayout::Platform, "{token}");
        assert_eq!(options.bar_handle, 3, "{token}");
        assert_eq!(options.dma_handle, 4, "{token}");
        assert_eq!(options.irq_handle, 5, "{token}");
    }
}

#[test]
fn the_pci_token_keeps_the_pci_layout() {
    for token in ["pci", "slots=pci"] {
        let options = Options::parse(with(&[token])).expect("pci options");
        assert_eq!(options.layout, GrantLayout::Pci, "{token}");
        assert_eq!(options.dma_handle, 5, "{token}");
        assert_eq!(options.irq_handle, 6, "{token}");
    }
}

#[test]
fn explicit_handles_select_the_layout() {
    let options = Options::parse(with(&["bar_handle=3", "dma_handle=4", "irq_handle=5"]))
        .expect("explicit options");
    assert_eq!(options.layout, GrantLayout::Explicit);
    assert_eq!(options.bar_handle, 3);
    assert_eq!(options.dma_handle, 4);
    assert_eq!(options.irq_handle, 5);
}

#[test]
fn explicit_handles_override_the_platform_token() {
    let options = Options::parse(with(&["platform", "dma_handle=4", "irq_handle=5"]))
        .expect("explicit override");
    assert_eq!(options.layout, GrantLayout::Explicit);
    assert_eq!(options.dma_handle, 4);
    assert_eq!(options.irq_handle, 5);
}

#[test]
fn test_mode_is_parsed() {
    let options = Options::parse(["bar=0", "bar_size=4096", "dma=4096", "irq=1", "test"])
        .expect("test options");
    assert!(options.test);
}

#[test]
fn missing_tokens_are_rejected() {
    assert_eq!(
        Options::parse(["bar=0", "dma=4096", "irq=1"]),
        Err(OptionsError::MissingBarSize)
    );
    assert_eq!(
        Options::parse(["bar=0", "bar_size=4096", "irq=1"]),
        Err(OptionsError::MissingDma)
    );
    assert_eq!(
        Options::parse(["bar=0", "bar_size=4096", "dma=4096"]),
        Err(OptionsError::MissingIrq)
    );
    assert_eq!(
        Options::parse(["bar=0", "bar_size=4096", "dma=4096", "irq=0"]),
        Err(OptionsError::MissingIrq)
    );
}

#[test]
fn malformed_handles_are_rejected() {
    assert_eq!(
        Options::parse(with(&["dma_handle=2", "irq_handle=5"])),
        Err(OptionsError::BadHandle)
    );
    assert_eq!(
        Options::parse(with(&["platform", "dma_handle=3", "irq_handle=5"])),
        Err(OptionsError::DuplicateHandle)
    );
    assert_eq!(
        Options::parse(with(&["dma_handle=16777216", "irq_handle=5"])),
        Err(OptionsError::BadHandle)
    );
    assert_eq!(
        Options::parse(with(&["dma_handle=x", "irq_handle=5"])),
        Err(OptionsError::BadNumber)
    );
}

#[test]
fn unknown_tokens_are_rejected() {
    assert_eq!(
        Options::parse(with(&["platfrom"])),
        Err(OptionsError::UnknownArgument)
    );
    assert_eq!(
        Options::parse(with(&["slots=isa"])),
        Err(OptionsError::BadSlots)
    );
    assert_eq!(
        Options::parse(["bar=x", "bar_size=4096", "dma=4096", "irq=1"]),
        Err(OptionsError::BadNumber)
    );
}
