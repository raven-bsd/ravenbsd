// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the Pi 5 image-layout descriptor and the xtask entry skeleton.

use super::*;

#[test]
fn layout_is_pi5_specific() {
    let layout = layout();
    assert_eq!(layout.profile, "pi5");
    assert_eq!(layout.boot_mode, BootMode::DirectKernel);
    assert_eq!(layout.dtb, "bcm2712-rpi-5-b.dtb");
    assert_eq!(layout.kernel_candidates, ["kernel_2712.img", "kernel8.img"]);
    assert!(layout.firmware_files.contains(&"start4.elf"));
    assert!(layout.firmware_files.contains(&"fixup4.dat"));
    assert!(layout.firmware_files.contains(&"bcm2712-rpi-5-b.dtb"));
    assert!(layout.config_txt.contains("kernel=kernel_2712.img"));
    assert!(
        layout
            .config_txt
            .contains("device_tree=bcm2712-rpi-5-b.dtb")
    );
    assert!(layout.config_txt.contains("arm_64bit=1"));
    assert_eq!(layout.cmdline, "console=ttyAMA0 loglevel=info");
    assert_eq!(layout.load_address, None);
    assert!(!layout.notes.is_empty());
}

#[test]
fn xtask_entry_point_describes_the_same_layout() {
    assert_eq!(crate::xtask_pi5::describe(), layout());
}
