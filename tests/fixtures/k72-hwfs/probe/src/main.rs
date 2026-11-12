// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Two-boot probe for the `hw-fs` qtest.
//!
//! The root service runs from the UFS root mounted at `/root`.  The first boot creates and
//! exercises a deterministic file set and records the payload checksum in `/root/etc/hw-fs-boot1`;
//! the second boot proves the payload survived with the exact bytes, the renamed name is present,
//! the unlinked and removed names are gone, and the surviving directory is a directory.
//!
//! Every operation is a normal `file:` operation through vfsd/fsd-ufs; the probe has no block or
//! driver access.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::{O_CREAT, O_RDWR, S_IFDIR, S_IFMT, S_IFREG};
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    const ROOT: &str = "file:/root";
    const PAYLOAD: &str = "file:/root/tmp/payload";
    const RENAMED: &str = "file:/root/tmp/payload.renamed";
    const VICTIM: &str = "file:/root/tmp/victim";
    const KEEP: &str = "/root/tmp/keep";
    const KEEP_FILE: &str = "file:/root/tmp/keep";
    const NEWDIR: &str = "/root/tmp/newdir";
    const NEWDIR_FILE: &str = "file:/root/tmp/newdir";
    const STATE: &str = "file:/root/etc/hw-fs-boot1";
    const PAYLOAD_LEN: usize = 12 * 1024;
    const STATE_LEN: usize = 16;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("hw-fs: error: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        match fs::stat(STATE) {
            Ok(_) => boot_two(),
            Err(Errno::ENOENT) => boot_one(),
            Err(error) => Err(format!("stat state: {}", error.description())),
        }
    }

    fn boot_one() -> Result<(), String> {
        raven_rt::println!("hw-fs: boot 1 start");
        if fs::stat(&format!("{ROOT}/etc")).is_err() {
            return Err("root filesystem is not mounted at /root".to_string());
        }
        raven_rt::println!("hw-fs: mount path=/root");

        fs::mkdir(KEEP, 0o755).map_err(|error| format!("mkdir keep: {}", error.description()))?;
        fs::mkdir(NEWDIR, 0o755)
            .map_err(|error| format!("mkdir newdir: {}", error.description()))?;
        raven_rt::println!("hw-fs: mkdir ok");

        let payload = payload_bytes();
        write_file(PAYLOAD, &payload)?;
        raven_rt::println!(
            "hw-fs: write payload bytes={} checksum={:#018x}",
            payload.len(),
            checksum(&payload)
        );

        let read_back = fs::read_to_vec(PAYLOAD)
            .map_err(|error| format!("read payload: {}", error.description()))?;
        if read_back.as_slice() != payload.as_slice() {
            return Err(format!(
                "payload read back {} bytes, expected {}",
                read_back.len(),
                payload.len()
            ));
        }
        raven_rt::println!(
            "hw-fs: read back ok checksum={:#018x}",
            checksum(&read_back)
        );

        fs::sync().map_err(|error| format!("fsync: {}", error.description()))?;
        raven_rt::println!("hw-fs: fsync ok");

        fs::rename("/root/tmp/payload", "/root/tmp/payload.renamed")
            .map_err(|error| format!("rename: {}", error.description()))?;
        if fs::stat(PAYLOAD).is_ok() {
            return Err("original name still exists after rename".to_string());
        }
        let renamed =
            fs::stat(RENAMED).map_err(|error| format!("stat renamed: {}", error.description()))?;
        if renamed.size != PAYLOAD_LEN as u64 || renamed.mode & S_IFMT != S_IFREG {
            return Err(format!(
                "renamed file is mode={:#o} size={}",
                renamed.mode, renamed.size
            ));
        }
        raven_rt::println!("hw-fs: rename payload -> payload.renamed ok");

        write_file(VICTIM, b"victim-payload")?;
        if fs::stat(VICTIM).is_err() {
            return Err("victim was not created".to_string());
        }
        fs::unlink("/root/tmp/victim")
            .map_err(|error| format!("unlink victim: {}", error.description()))?;
        if fs::stat(VICTIM).is_ok() {
            return Err("victim still exists after unlink".to_string());
        }
        raven_rt::println!("hw-fs: unlink victim ok");

        write_file("file:/root/tmp/newdir/inside", b"inside")?;
        match fs::rmdir(NEWDIR) {
            Err(Errno::ENOTEMPTY) => {
                raven_rt::println!(
                    "hw-fs: rmdir non-empty rejected errno={}",
                    Errno::ENOTEMPTY.0
                );
            }
            other => return Err(format!("rmdir of a non-empty directory returned {other:?}")),
        }
        fs::unlink("/root/tmp/newdir/inside")
            .map_err(|error| format!("unlink inside: {}", error.description()))?;
        fs::rmdir(NEWDIR).map_err(|error| format!("rmdir empty: {}", error.description()))?;
        if fs::stat(NEWDIR_FILE).is_ok() {
            return Err("newdir still exists after rmdir".to_string());
        }
        raven_rt::println!("hw-fs: rmdir empty ok");

        let keep =
            fs::stat(KEEP_FILE).map_err(|error| format!("stat keep: {}", error.description()))?;
        if keep.mode & S_IFMT != S_IFDIR {
            return Err(format!("keep mode is {:#o}", keep.mode));
        }

        let mut state = [0u8; STATE_LEN];
        state[0..8].copy_from_slice(&checksum(&payload).to_le_bytes());
        state[8..16].copy_from_slice(&(PAYLOAD_LEN as u64).to_le_bytes());
        write_file(STATE, &state)?;
        fs::sync().map_err(|error| format!("state fsync: {}", error.description()))?;
        raven_rt::println!("hw-fs: boot 1 state written");
        raven_rt::println!("hw-fs: boot 1 ok");
        Ok(())
    }

    fn boot_two() -> Result<(), String> {
        raven_rt::println!("hw-fs: boot 2 start");
        let state = fs::read_to_vec(STATE)
            .map_err(|error| format!("read state: {}", error.description()))?;
        if state.len() != STATE_LEN {
            return Err(format!("state is {} bytes", state.len()));
        }
        let stored = u64::from_le_bytes(
            state
                .get(0..8)
                .ok_or("state checksum")?
                .try_into()
                .map_err(|_| "state checksum")?,
        );
        let expected_len = u64::from_le_bytes(
            state
                .get(8..16)
                .ok_or("state length")?
                .try_into()
                .map_err(|_| "state length")?,
        );
        if expected_len != PAYLOAD_LEN as u64 {
            return Err(format!("state length {expected_len}"));
        }

        let payload = fs::read_to_vec(RENAMED)
            .map_err(|error| format!("read renamed: {}", error.description()))?;
        let expected = payload_bytes();
        if payload.as_slice() != expected.as_slice() {
            return Err(format!(
                "surviving payload is {} bytes, expected {}",
                payload.len(),
                expected.len()
            ));
        }
        if checksum(&payload) != stored {
            return Err(format!(
                "checksum {:#018x} != stored {stored:#018x}",
                checksum(&payload)
            ));
        }
        raven_rt::println!(
            "hw-fs: boot 2 payload survived bytes={} checksum={:#018x}",
            payload.len(),
            stored
        );

        if fs::stat(PAYLOAD).is_ok() {
            return Err("original payload name survived".to_string());
        }
        if fs::stat(VICTIM).is_ok() {
            return Err("unlinked victim survived".to_string());
        }
        if fs::stat(NEWDIR_FILE).is_ok() {
            return Err("removed newdir survived".to_string());
        }
        let keep =
            fs::stat(KEEP_FILE).map_err(|error| format!("stat keep: {}", error.description()))?;
        if keep.mode & S_IFMT != S_IFDIR {
            return Err("surviving keep is not a directory".to_string());
        }
        raven_rt::println!("hw-fs: boot 2 unlinked gone");
        raven_rt::println!("hw-fs: boot 2 ok");
        Ok(())
    }

    fn payload_bytes() -> Vec<u8> {
        let mut data = vec![0u8; PAYLOAD_LEN];
        for (index, byte) in data.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(31).wrapping_add(7);
        }
        data
    }

    fn checksum(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
        let mut file = fs::open(path, O_RDWR | O_CREAT)
            .map_err(|error| format!("open {path}: {}", error.description()))?;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let end = (offset + 4096).min(bytes.len());
            let chunk = bytes.get(offset..end).ok_or("write chunk")?;
            let mut written = 0usize;
            while written < chunk.len() {
                let part = chunk.get(written..).ok_or("write part")?;
                let count = file
                    .write(part)
                    .map_err(|error| format!("write {path}: {}", error.description()))?;
                if count == 0 {
                    return Err(format!("write {path}: no progress"));
                }
                written += count;
            }
            offset = end;
        }
        file.close()
            .map_err(|error| format!("close {path}: {}", error.description()))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
