// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the EMMC2 launch-option parser.

use super::*;

#[test]
fn default_uses_the_platform_handle_layout() {
    let options = Options::parse(&[]).expect("defaults");
    assert!(!options.test);
    assert_eq!(options.mmio, Handle::new(3, 0));
    assert_eq!(options.dma, Handle::new(4, 0));
    assert_eq!(options.irq, Handle::new(5, 0));
    assert_eq!(options.mmio_size, MMIO_SIZE);
    assert_eq!(options.dma_size, DMA_BYTES);
    assert_eq!(options.window.bus_base, 0xc000_0000);
    assert_eq!(options.window.cpu_base, 0);
    assert_eq!(options.window.size, 0x4000_0000);
    assert_eq!(options.clock_hz, None);
    assert_eq!(options.disk, None);
}

#[test]
fn devd_platform_tokens_map_to_handles_and_sizes() {
    let options = Options::parse(&[
        "emmc2",
        "bar=0",
        "bar_size=256",
        "dma=1048576",
        "irq=126",
        "clock=50000000",
        "window_bus=0xc0000000",
        "window_cpu=0",
        "window_size=0x40000000",
    ])
    .expect("platform tokens");
    assert!(!options.test);
    assert_eq!(options.mmio, Handle::new(3, 0));
    assert_eq!(options.dma, Handle::new(4, 0));
    assert_eq!(options.irq, Handle::new(5, 0));
    assert_eq!(options.mmio_size, 256);
    assert_eq!(options.dma_size, 1_048_576);
    assert_eq!(options.irq_line, Some(126));
    assert_eq!(options.clock_hz, Some(50_000_000));
}

#[test]
fn explicit_handles_override_the_layout() {
    let options = Options::parse(&[
        "mmio_handle=4",
        "dma_handle=5",
        "irq_handle=6",
        "mmio_size=4096",
        "dma_size=65536",
    ])
    .expect("explicit handles");
    assert_eq!(options.mmio, Handle::new(4, 0));
    assert_eq!(options.dma, Handle::new(5, 0));
    assert_eq!(options.irq, Handle::new(6, 0));
    assert_eq!(options.mmio_size, 4096);
    assert_eq!(options.dma_size, 65_536);
}

#[test]
fn test_mode_and_disk_path_are_parsed() {
    let options = Options::parse(&["test", "disk=initfs:/etc/pi4-root.img"]).expect("test");
    assert!(options.test);
    assert!(!options.persist);
    assert_eq!(options.disk.as_deref(), Some("initfs:/etc/pi4-root.img"));
}

#[test]
fn persist_keeps_the_test_disk_alive() {
    let options =
        Options::parse(&["test", "persist", "disk=initfs:/etc/pi4-root.img"]).expect("persist");
    assert!(options.test);
    assert!(options.persist);
}

#[test]
fn path_shaped_argv_zero_is_ignored() {
    let options = Options::parse(&["/sbin/emmc2", "platform", "bar=0"]).expect("argv0");
    assert_eq!(options.mmio, Handle::new(3, 0));
}

#[test]
fn bad_numbers_and_unknown_tokens_are_rejected() {
    assert!(Options::parse(&["dma=x"]).is_err());
    assert!(Options::parse(&["bar_size=0"]).is_err());
    assert!(Options::parse(&["bar=1"]).is_err());
    assert!(Options::parse(&["mmio_handle=2"]).is_err());
    assert!(Options::parse(&["clock=fast"]).is_err());
    assert!(Options::parse(&["surprise"]).is_err());
}

#[test]
fn zero_windows_are_rejected() {
    assert!(Options::parse(&["mmio_size=0"]).is_err());
    assert!(Options::parse(&["dma_size=0"]).is_err());
    assert!(Options::parse(&["dma=0"]).is_err());
}
