// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compile-time demo for `raven_rt::spawn`.
//!
//! The example packs a minimal `/child` into a local initfs image and calls
//! `spawn_with`/`proc_wait`. It is a template for the S24/M2 integration: after S24 lands, `/init`
//! will use the kernel-mapped initfs instead of this local copy, and the child will be the real
//! `/child` program from the image.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_initfs::{Initfs, crc32};
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        let child = child_elf();
        let image = initfs_image(&child);
        let initfs = match Initfs::parse(&image) {
            Ok(initfs) => initfs,
            Err(_) => {
                raven_rt::eprintln!("spawn_demo: invalid initfs");
                return 1;
            }
        };
        let mut sys = RtSys;
        let options = SpawnOptions {
            args: &["/child"],
            env: &[],
        };
        let child = match spawn_with(&initfs, "/child", &options, &mut sys) {
            Ok(child) => child,
            Err(error) => {
                raven_rt::eprintln!("spawn_demo: {:?}", error);
                return 1;
            }
        };
        let mut code = -1;
        if let Err(error) = sys.proc_wait(child, &mut code) {
            raven_rt::eprintln!("spawn_demo: wait: {}", error.description());
            return 1;
        }
        raven_rt::println!("spawn_demo: child exit {}", code);
        if code == 0 { 0 } else { 1 }
    }

    /// A one-segment ET_EXEC child whose entry exits with status 0 on x86-64.
    fn child_elf() -> [u8; 4096] {
        let mut elf = [0u8; 4096];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        put_u16(&mut elf, 16, 2);
        put_u16(&mut elf, 18, 62);
        put_u32(&mut elf, 20, 1);
        put_u64(&mut elf, 24, 0x0040_0100);
        put_u64(&mut elf, 32, 64);
        put_u16(&mut elf, 52, 64);
        put_u16(&mut elf, 54, 56);
        put_u16(&mut elf, 56, 1);
        put_u32(&mut elf, 64, 1);
        put_u32(&mut elf, 68, 5);
        put_u64(&mut elf, 72, 0);
        put_u64(&mut elf, 80, 0x0040_0000);
        put_u64(&mut elf, 88, 0);
        put_u64(&mut elf, 96, 0x1000);
        put_u64(&mut elf, 104, 0x1000);
        put_u64(&mut elf, 112, 0x1000);
        elf[0x100..0x10b].copy_from_slice(&[
            0xb8, 0x01, 0x00, 0x00, 0x00, 0x31, 0xff, 0x0f, 0x05, 0x0f, 0x0b,
        ]);
        elf
    }

    /// Pack the child at `/child` in the C7 initfs layout.
    fn initfs_image(child: &[u8; 4096]) -> [u8; 8192] {
        let mut image = [0u8; 8192];
        image[0..8].copy_from_slice(b"RAVENFS1");
        put_u32(&mut image, 8, 1);
        put_u32(&mut image, 12, 64);
        put_u32(&mut image, 16, 1);
        put_u32(&mut image, 20, 48);
        put_u64(&mut image, 24, 112);
        put_u64(&mut image, 32, 6);
        put_u64(&mut image, 40, 8192);
        put_u32(&mut image, 64, 0);
        put_u32(&mut image, 68, 6);
        put_u64(&mut image, 72, 4096);
        put_u64(&mut image, 80, 4096);
        put_u32(&mut image, 88, 0o755);
        put_u32(&mut image, 92, crc32(child));
        image[112..118].copy_from_slice(b"/child");
        image[4096..8192].copy_from_slice(child);
        image
    }

    fn put_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(data: &mut [u8], offset: usize, value: u64) {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
