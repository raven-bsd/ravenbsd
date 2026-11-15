// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reading files from the volume the loader was started from (CONTRACTS C3).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use uefi::fs::FileSystem;
use uefi::{CStr16, cstr16};

/// Largest file the loader accepts, in bytes (256 MiB).
pub const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;

/// `\raven\kernel.elf`.
pub const KERNEL_PATH: &CStr16 = cstr16!("\\raven\\kernel.elf");
/// `\raven\initfs.img`.
pub const INITFS_PATH: &CStr16 = cstr16!("\\raven\\initfs.img");
/// `\raven\boot.toml`.
pub const BOOT_TOML_PATH: &CStr16 = cstr16!("\\raven\\boot.toml");
/// `\raven\slot-a\kernel.elf`.
pub const SLOT_A_KERNEL_PATH: &CStr16 = cstr16!("\\raven\\slot-a\\kernel.elf");
/// `\raven\slot-a\initfs.img`.
pub const SLOT_A_INITFS_PATH: &CStr16 = cstr16!("\\raven\\slot-a\\initfs.img");
/// `\raven\slot-b\kernel.elf`.
pub const SLOT_B_KERNEL_PATH: &CStr16 = cstr16!("\\raven\\slot-b\\kernel.elf");
/// `\raven\slot-b\initfs.img`.
pub const SLOT_B_INITFS_PATH: &CStr16 = cstr16!("\\raven\\slot-b\\initfs.img");

/// True when `path` exists on the boot volume.
pub fn exists(fs: &mut FileSystem, path: &CStr16) -> Result<bool, String> {
    fs.try_exists(path)
        .map_err(|error| format!("cannot probe {path}: {error}"))
}

/// Read a whole file, rejecting files larger than [`MAX_FILE_SIZE`].
pub fn read_file(fs: &mut FileSystem, path: &CStr16) -> Result<Vec<u8>, String> {
    let info = fs
        .metadata(path)
        .map_err(|error| format!("cannot stat {path}: {error}"))?;
    let size = info.file_size();
    if size > MAX_FILE_SIZE {
        return Err(format!(
            "{path} is {size} bytes, over the {MAX_FILE_SIZE} byte limit"
        ));
    }
    fs.read(path)
        .map_err(|error| format!("cannot read {path}: {error}"))
}

/// Replace a file with `data`, creating it when absent.
pub fn write_file(fs: &mut FileSystem, path: &CStr16, data: &[u8]) -> Result<(), String> {
    fs.write(path, data)
        .map_err(|error| format!("cannot write {path}: {error}"))
}
