// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for platform resource selection.

use alloc::string::ToString;
use alloc::vec;

use super::*;
use crate::dtb::{DeviceSpec, GicInterrupt, InterruptKind, RegWindow};

fn device() -> DeviceSpec {
    DeviceSpec {
        path: "/scb/fake@fd580000".to_string(),
        compatibles: vec!["raven,fake-platform".to_string()],
        regs: vec![
            RegWindow {
                base: 0xfd58_0000,
                size: 0x1_0000,
            },
            RegWindow {
                base: 0xfd59_0000,
                size: 0x1000,
            },
            RegWindow {
                base: 0xfd5a_0000,
                size: 0x2000,
            },
        ],
        dma: vec![],
        interrupts: vec![GicInterrupt {
            kind: InterruptKind::Spi,
            number: 157,
            trigger: 4,
        }],
    }
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn defaults_to_the_first_window_without_resources() {
    let plan = plan(&device(), &[]).expect("plan");
    assert_eq!(plan.bars, [0]);
    assert_eq!(plan.regs[0].base, 0xfd58_0000);
    assert_eq!(plan.dma, None);
    assert_eq!(plan.irq, None);
    assert_eq!(
        plan.tokens(),
        ["bar=0".to_string(), "bar_size=65536".to_string()]
    );
}

#[test]
fn selects_the_requested_window_and_resources() {
    let plan = plan(&device(), &args(&["bar=1", "resources=dma+irq"])).expect("plan");
    assert_eq!(plan.bars, [1]);
    assert_eq!(plan.regs[0].base, 0xfd59_0000);
    assert_eq!(plan.dma, Some(PLATFORM_DMA_LEN));
    assert_eq!(plan.irq.expect("irq").intid(), 189);
    assert_eq!(
        plan.tokens(),
        [
            "bar=1".to_string(),
            "bar_size=4096".to_string(),
            "dma=1048576".to_string(),
            "irq=189".to_string(),
        ]
    );
}

#[test]
fn supports_the_bare_resource_tokens_and_custom_dma_size() {
    let plan = plan(&device(), &args(&["dma", "irq", "dma_size=4096", "extra"])).expect("plan");
    assert_eq!(plan.dma, Some(4096));
    assert!(plan.irq.is_some());
    assert_eq!(plan.rest, ["extra".to_string()]);
    let tokens = plan.tokens();
    assert!(tokens.contains(&"dma=4096".to_string()));
    assert!(tokens.contains(&"extra".to_string()));
}

#[test]
fn resources_none_clears_previous_requests() {
    let plan = plan(&device(), &args(&["resources=dma+irq", "resources=none"])).expect("plan");
    assert_eq!(plan.dma, None);
    assert_eq!(plan.irq, None);
}

#[test]
fn rejects_a_missing_window() {
    assert_eq!(
        plan(&device(), &args(&["bar=3"])),
        Err(PlatformError::BarOutOfRange(3))
    );
}

#[test]
fn selects_multiple_windows_in_declaration_order() {
    let plan = plan(&device(), &args(&["mmio=2,0,1", "resources=dma"])).expect("plan");
    assert_eq!(plan.bars, [2, 0, 1]);
    assert_eq!(
        plan.regs.iter().map(|reg| reg.base).collect::<Vec<_>>(),
        [0xfd5a_0000, 0xfd58_0000, 0xfd59_0000]
    );
    assert_eq!(
        plan.grants(),
        vec![
            Grant::Bar {
                index: 2,
                base: 0xfd5a_0000,
                size: 0x2000,
            },
            Grant::Bar {
                index: 0,
                base: 0xfd58_0000,
                size: 0x1_0000,
            },
            Grant::Bar {
                index: 1,
                base: 0xfd59_0000,
                size: 0x1000,
            },
            Grant::Dma {
                len: PLATFORM_DMA_LEN,
            },
        ]
    );
    let tokens = plan.tokens();
    assert!(tokens.contains(&"bar=2".to_string()));
    assert!(tokens.contains(&"bar_size=8192".to_string()));
    assert!(tokens.contains(&"dma=1048576".to_string()));
}

#[test]
fn repeated_bar_selectors_are_ordered() {
    let plan = plan(&device(), &args(&["bar=1", "bar=0"])).expect("plan");
    assert_eq!(plan.bars, [1, 0]);
    assert_eq!(
        plan.grants(),
        vec![
            Grant::Bar {
                index: 1,
                base: 0xfd59_0000,
                size: 0x1000,
            },
            Grant::Bar {
                index: 0,
                base: 0xfd58_0000,
                size: 0x1_0000,
            },
        ]
    );
}

#[test]
fn rejects_duplicate_windows() {
    assert_eq!(
        plan(&device(), &args(&["mmio=0,0"])),
        Err(PlatformError::DuplicateWindow(0))
    );
    assert_eq!(
        plan(&device(), &args(&["bar=1", "bar=1"])),
        Err(PlatformError::DuplicateWindow(1))
    );
}

#[test]
fn rejects_a_missing_window_in_a_multi_selection() {
    assert_eq!(
        plan(&device(), &args(&["mmio=0,3"])),
        Err(PlatformError::BarOutOfRange(3))
    );
}

#[test]
fn keeps_named_handle_tokens_in_rest() {
    let plan = plan(
        &device(),
        &args(&[
            "mmio=0,1,2",
            "resources=dma",
            "dma_size=256",
            "dma=6:256",
            "mailbox=3:64",
        ]),
    )
    .expect("plan");
    assert_eq!(plan.dma, Some(PLATFORM_PAGE_SIZE));
    assert!(plan.rest.contains(&"dma=6:256".to_string()));
    assert!(plan.rest.contains(&"mailbox=3:64".to_string()));
}

#[test]
fn rejects_an_irq_when_the_node_has_none() {
    let mut device = device();
    device.interrupts.clear();
    assert_eq!(
        plan(&device, &args(&["resources=irq"])),
        Err(PlatformError::MissingInterrupt)
    );
}

#[test]
fn rejects_malformed_selectors() {
    assert_eq!(
        plan(&device(), &args(&["bar=nope"])),
        Err(PlatformError::BadArgument)
    );
    for value in ["mmio=", "mmio=0,,1", "mmio=0,x"] {
        assert_eq!(
            plan(&device(), &args(&[value])),
            Err(PlatformError::BadArgument),
            "{value}"
        );
    }
    assert_eq!(
        plan(&device(), &args(&["resources=dma+clock"])),
        Err(PlatformError::BadArgument)
    );
    assert_eq!(
        plan(&device(), &args(&["dma_size=0"])),
        Err(PlatformError::BadDmaLength)
    );
}

#[test]
fn ppi_uses_the_gic_ppi_base() {
    let mut device = device();
    device.interrupts = vec![GicInterrupt {
        kind: InterruptKind::Ppi,
        number: 5,
        trigger: 4,
    }];
    let plan = plan(&device, &args(&["resources=irq"])).expect("plan");
    assert_eq!(plan.irq.expect("irq").intid(), 21);
}

#[test]
fn rounds_a_short_dma_length_up_to_a_page() {
    let plan = plan(&device(), &args(&["resources=dma", "dma_size=1000"])).expect("plan");
    assert_eq!(plan.dma, Some(PLATFORM_PAGE_SIZE));
    assert!(plan.tokens().contains(&"dma=4096".to_string()));
    assert_eq!(
        plan.grants()[1],
        Grant::Dma {
            len: PLATFORM_PAGE_SIZE,
        }
    );
}

#[test]
fn grants_follow_the_declared_resources_in_transfer_order() {
    let plan = plan(&device(), &args(&["bar=1", "resources=dma+irq"])).expect("plan");
    assert_eq!(
        plan.grants(),
        vec![
            Grant::Bar {
                index: 1,
                base: 0xfd59_0000,
                size: 0x1000,
            },
            Grant::Dma {
                len: PLATFORM_DMA_LEN,
            },
            Grant::Irq { intid: 189 },
        ]
    );
}

#[test]
fn grants_omit_resources_the_manifest_did_not_ask_for() {
    let bar_only = plan(&device(), &[]).expect("plan");
    assert_eq!(
        bar_only.grants(),
        vec![Grant::Bar {
            index: 0,
            base: 0xfd58_0000,
            size: 0x1_0000,
        }]
    );

    let cleared = plan(&device(), &args(&["resources=dma", "resources=none"])).expect("plan");
    assert_eq!(cleared.grants().len(), 1);
}

#[test]
fn grants_are_selected_only_after_the_plan_is_valid() {
    assert!(plan(&device(), &args(&["bar=7", "resources=dma+irq"])).is_err());
    assert!(plan(&device(), &args(&["resources=dma+irq", "dma_size=0"])).is_err());
}
