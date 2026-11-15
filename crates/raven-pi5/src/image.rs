// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side description of the Pi 5 boot image.
//!
//! B54 does not build a bootable image. This module records the file set, the firmware
//! configuration and the questions that only a board log can answer. The values come from the
//! official `rpi-firmware` package and the Pi 5 architecture plan; every hardware-dependent item
//! is marked UNVERIFIED in [`NOTES`].

/// The profile name.
pub const PROFILE: &str = "pi5";
/// The board device tree in the firmware package.
pub const DTB: &str = "bcm2712-rpi-5-b.dtb";
/// The kernel image names the Pi 5 firmware package ships. The firmware's selection and the load
/// address are UNVERIFIED.
pub const KERNEL_CANDIDATES: &[&str] = &["kernel_2712.img", "kernel8.img"];
/// The files the host-side layout lists for the FAT boot partition. The exact required subset for
/// the Pi 5 EEPROM bootloader is UNVERIFIED.
pub const FIRMWARE_FILES: &[&str] = &[
    "start4.elf",
    "fixup4.dat",
    "bcm2712-rpi-5-b.dtb",
    "kernel_2712.img",
    "config.txt",
];
/// The proposed `config.txt` body.
pub const CONFIG_TXT: &str = "\
arm_64bit=1
kernel=kernel_2712.img
device_tree=bcm2712-rpi-5-b.dtb
enable_uart=1
uart_2ndstage=1
";
/// The proposed kernel command line; it matches `profiles/pi5.toml`.
pub const CMDLINE: &str = "console=ttyAMA0 loglevel=info";

/// The boot path the image is being prepared for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMode {
    /// The VideoCore/EEPROM firmware loads a flat kernel image and passes the DTB.
    DirectKernel,
    /// A UEFI payload starts the loader.
    Uefi,
}

/// The host-side Pi 5 image layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLayout {
    /// Profile name.
    pub profile: &'static str,
    /// Selected boot mode.
    pub boot_mode: BootMode,
    /// Kernel image names to try, in firmware preference order.
    pub kernel_candidates: &'static [&'static str],
    /// Board device tree.
    pub dtb: &'static str,
    /// Files for the FAT boot partition.
    pub firmware_files: &'static [&'static str],
    /// `config.txt` body.
    pub config_txt: &'static str,
    /// Kernel command line.
    pub cmdline: &'static str,
    /// Load address, only when a documented value exists. The Pi 5 value is UNVERIFIED.
    pub load_address: Option<u64>,
    /// Hardware-dependent questions and provenance notes.
    pub notes: &'static [&'static str],
}

/// The notes repeated in the image descriptor.
pub const NOTES: &[&str] = &[
    "kernel_2712.img is present in the official firmware package; kernel8.img is the fallback. The firmware's selection is UNVERIFIED.",
    "start4.elf/fixup4.dat/bootcode.bin are present in the firmware package; the exact Pi 5 EEPROM boot set is UNVERIFIED.",
    "The Pi 5 kernel load address and entry EL are UNVERIFIED; this descriptor encodes no load_address.",
    "The BCM2712 UART clock and the debug-connector pinout are UNVERIFIED.",
    "No RP1 firmware blob is included or redistributed; B55 owns the firmware load.",
    "This is a host-side descriptor only; B54 does not build a bootable image.",
];

/// Return the Pi 5 image layout descriptor.
#[must_use]
pub const fn layout() -> ImageLayout {
    ImageLayout {
        profile: PROFILE,
        boot_mode: BootMode::DirectKernel,
        kernel_candidates: KERNEL_CANDIDATES,
        dtb: DTB,
        firmware_files: FIRMWARE_FILES,
        config_txt: CONFIG_TXT,
        cmdline: CMDLINE,
        load_address: None,
        notes: NOTES,
    }
}

#[cfg(test)]
#[path = "image_ut.rs"]
mod tests;
