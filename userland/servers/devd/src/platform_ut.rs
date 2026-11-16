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
    assert_eq!(plan.bar, 0);
    assert_eq!(plan.reg.base, 0xfd58_0000);
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
    assert_eq!(plan.bar, 1);
    assert_eq!(plan.reg.base, 0xfd59_0000);
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
        plan(&device(), &args(&["bar=2"])),
        Err(PlatformError::BarOutOfRange(2))
    );
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
