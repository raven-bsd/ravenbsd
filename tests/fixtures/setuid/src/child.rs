// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3b setuid/setgid child helper. Prints the credentials and auxiliary vector the kernel
//! installed for the image.
//!
//! The `auxv` label is K526's two-exec proof: the first child reads its spawn `AT_RANDOM` and
//! re-execs itself through `PROC_EXEC` carrying the previous bytes in argv; the replacement child
//! compares the new vector value and prints `distinct=1`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;
use raven_setuid::self_info;

entry!(main);

const AT_UID: usize = 11;
const AT_EUID: usize = 12;
const AT_GID: usize = 13;
const AT_EGID: usize = 14;
const AT_SECURE: usize = 23;
const AT_RANDOM: usize = 25;

fn main(args: raven_rt::Args) -> i32 {
    let label = args.get(1).unwrap_or("?");
    let random = random_bytes(&args);
    if label == "auxv" {
        return auxv_row(&args, &random);
    }
    let mut info = [0u64; 8];
    if self_info(&mut info).is_err() {
        println!("setuid-child: fail self-info");
        return 1;
    }
    println!(
        "setuid-child: mode={} uid={} euid={} gid={} egid={} auxv-uid={} auxv-euid={} auxv-gid={} auxv-egid={} secure={} random-nonzero={}",
        label,
        info[4],
        info[5],
        info[6],
        info[7],
        args.auxv(AT_UID).unwrap_or(usize::MAX),
        args.auxv(AT_EUID).unwrap_or(usize::MAX),
        args.auxv(AT_GID).unwrap_or(usize::MAX),
        args.auxv(AT_EGID).unwrap_or(usize::MAX),
        args.auxv(AT_SECURE).unwrap_or(usize::MAX),
        random.iter().any(|byte| *byte != 0) as usize,
    );
    0
}

fn random_bytes(args: &raven_rt::Args) -> [u8; 16] {
    let address = args.auxv(AT_RANDOM).unwrap_or(0);
    if address == 0 {
        return [0u8; 16];
    }
    // SAFETY: AT_RANDOM names 16 readable bytes on the process stack.
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, 16) };
    let mut out = [0u8; 16];
    out.copy_from_slice(bytes);
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn auxv_row(args: &raven_rt::Args, random: &[u8; 16]) -> i32 {
    let now = hex(random);
    let nonzero = random.iter().any(|byte| *byte != 0);
    if let Some(previous) = args.get(2) {
        let previous_nonzero = previous.bytes().any(|byte| byte != b'0');
        let now_nonzero = random.iter().any(|byte| *byte != 0);
        let distinct = previous != now;
        println!(
            "auxv-spawn: nonzero={} distinct={} prev={} now={}",
            (previous_nonzero && now_nonzero) as usize,
            distinct as usize,
            previous,
            now
        );
        return if previous_nonzero && now_nonzero && distinct {
            0
        } else {
            1
        };
    }
    // First exec from the spawner: carry these bytes into the same-pid exec and compare there.
    match exec_self(&now) {
        Ok(()) => {
            println!("auxv-spawn: fail exec returned");
            1
        }
        Err(error) => {
            println!("auxv-spawn: fail exec {error:?}");
            1
        }
    }
}

/// Read the child's own initfs image and replace the current process with it.
///
/// `raven_rt::exec::exec_image` bounds images at 1 MiB; the fixture child is larger, so the
/// request is built directly and `PROC_EXEC` applies the kernel's 16 MiB bound.
fn exec_self(previous: &str) -> Result<(), raven_abi::errno::Errno> {
    let client = Client::new();
    let handle = client.open("initfs:/bin/child", u32::from(OpenFlags::READ.to_wire()))?;
    let image =
        raven_rt::spawn::read_file_handle(handle).map_err(|_| raven_abi::errno::Errno::EIO)?;
    let _ = client.close(handle);

    let path = b"/bin/child";
    let arguments: [&[u8]; 3] = [path, b"auxv", previous.as_bytes()];
    let mut strings = Vec::new();
    let mut offsets = Vec::new();
    for argument in arguments {
        offsets.push(u64::try_from(strings.len()).map_err(|_| raven_abi::errno::Errno::E2BIG)?);
        strings.extend_from_slice(argument);
        strings.push(0);
    }
    let base = strings.as_ptr() as u64;
    let pointers: Vec<u64> = offsets
        .iter()
        .map(|offset| base.saturating_add(*offset))
        .collect();
    let request = raven_rt::exec::ExecRequest {
        image: image.as_ptr() as u64,
        image_len: image.len() as u64,
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        argv: pointers.as_ptr() as u64,
        argc: pointers.len() as u64,
        envp: 0,
        envc: 0,
        close_list: 0,
        close_count: 0,
        flags: 0,
    };
    raven_rt::exec::exec_raw(&request)
}
