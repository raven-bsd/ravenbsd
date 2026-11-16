// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the platd capability plan.

use alloc::format;
use alloc::string::ToString;

use super::*;

fn valid() -> [&'static str; 4] {
    ["mailbox=3:64", "gpio=4:180", "rng=5:40", "dma=6:256"]
}

#[test]
fn parses_the_four_declared_windows() {
    let plan = parse(&valid()).expect("plan");
    assert_eq!(
        plan.mailbox,
        Window {
            handle: 3,
            len: MAILBOX_LEN
        }
    );
    assert_eq!(
        plan.gpio,
        Window {
            handle: 4,
            len: GPIO_LEN
        }
    );
    assert_eq!(
        plan.rng,
        Window {
            handle: 5,
            len: RNG_LEN
        }
    );
    assert_eq!(
        plan.dma,
        Window {
            handle: 6,
            len: DMA_LEN
        }
    );
}

#[test]
fn ignores_the_p8_launcher_and_fake_tokens() {
    let plan = parse(&[
        "bar=0",
        "bar_size=65536",
        "dma=1048576",
        "irq=189",
        "fake",
        "mailbox=3:64",
        "gpio=4:180",
        "rng=5:40",
        "dma=6:256",
    ])
    .expect("plan");
    assert_eq!(plan.dma.handle, 6);
    assert_eq!(plan.dma.len, DMA_LEN);
}

#[test]
fn rejects_each_missing_window() {
    let cases = [("mailbox", 0), ("gpio", 1), ("rng", 2), ("dma", 3)];
    for (name, index) in cases {
        let mut args = valid().to_vec();
        args.remove(index);
        let error = parse(&args).expect_err("missing window");
        assert_eq!(error, ResourceError::Missing(name));
    }
}

#[test]
fn rejects_short_windows() {
    let plan = [
        ("mailbox=3:63", "mailbox", 63, MAILBOX_LEN),
        ("gpio=4:179", "gpio", 179, GPIO_LEN),
        ("rng=5:39", "rng", 39, RNG_LEN),
        ("dma=6:255", "dma", 255, DMA_LEN),
    ];
    for (replacement, name, len, minimum) in plan {
        let mut args = valid();
        let slot = match name {
            "mailbox" => 0,
            "gpio" => 1,
            "rng" => 2,
            _ => 3,
        };
        args[slot] = replacement;
        assert_eq!(
            parse(&args).expect_err("short window"),
            ResourceError::ShortWindow { name, len, minimum }
        );
    }
}

#[test]
fn rejects_duplicate_handles() {
    assert_eq!(
        parse(&["mailbox=3:64", "gpio=3:180", "rng=5:40", "dma=6:256"]).expect_err("duplicate"),
        ResourceError::DuplicateHandle(3)
    );
}

#[test]
fn rejects_reserved_handles() {
    for handle in 0..FIRST_GRANTED_HANDLE {
        let value = format!("mailbox={handle}:64");
        let args = [value.as_str(), "gpio=4:180", "rng=5:40", "dma=6:256"];
        assert_eq!(
            parse(&args).expect_err("reserved"),
            ResourceError::ReservedHandle(handle)
        );
    }
}

#[test]
fn rejects_unknown_arguments() {
    assert_eq!(
        parse(&[
            "mailbox=3:64",
            "gpio=4:180",
            "rng=5:40",
            "dma=6:256",
            "clock=12",
        ])
        .expect_err("unknown"),
        ResourceError::Unknown("clock=12".to_string())
    );
}

#[test]
fn rejects_malformed_values() {
    let cases = [
        ("mailbox=3", 0),
        ("mailbox=x:64", 0),
        ("mailbox=3:x", 0),
        ("dma=6:0", 3),
    ];
    for (value, slot) in cases {
        let mut args = valid();
        args[slot] = value;
        assert!(
            matches!(parse(&args), Err(ResourceError::BadArgument(_))),
            "{value} was accepted"
        );
    }
}
