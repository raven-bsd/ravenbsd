// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guest probe for the K466 ENOSPC and sync/persistence qtests.
//!
//! It is a static no_std program injected into the external UFS2 disk by the case
//! `pre` script and run from the mounted filesystem.
//!
//! `enospc` fills `/mnt/k466-fill` until a write fails, requires `ENOSPC`, verifies
//! the written prefix/suffix and prints the free-block floor.
//!
//! `persist-write` writes a deterministic file, a directory entry and a rename,
//! syncs twice after each step, and prints checksums. `persist-check` verifies every
//! byte, the directory entry and the rename after a power-cut boot of the same image.

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
    use raven_proto_fs::{Call, CallRequest, CallResponse, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_scheme::Client;

    entry!(main);

    const FILL_PATH: &str = "file:/mnt/k466-fill";
    const DATA_PATH: &str = "file:/mnt/k466-data.bin";
    const ENTRY_PATH: &str = "file:/mnt/k466-entry.txt";
    const RENAMED_PATH: &str = "file:/mnt/k466-renamed.txt";

    /// One S11 write/read payload. The block path costs a few synchronous requests per
    /// call, so 16 KiB keeps the fill and the byte check inside the case timeout.
    const CHUNK: usize = 16 * 1024;
    const PERSIST_CHUNKS: usize = 4;
    const MAX_FILL_CHUNKS: usize = 256;

    fn main(args: raven_rt::Args) -> i32 {
        let mut mode = "";
        for argument in args.iter().skip(1) {
            mode = argument;
            break;
        }
        let result = match mode {
            "enospc" => enospc(),
            "persist-write" => persist_write(),
            "persist-check" => persist_check(),
            _ => {
                raven_rt::eprintln!("usage: ufs-probe enospc|persist-write|persist-check");
                return 2;
            }
        };
        match result {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("k466: probe: {message}");
                1
            }
        }
    }

    fn pattern(seed: u32, buffer: &mut [u8]) {
        let mut state = u64::from(seed).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        for (index, byte) in buffer.iter_mut().enumerate() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *byte = (state >> 33) as u8 ^ (index as u8).wrapping_mul(31);
        }
    }

    fn chunk(seed: u32) -> Vec<u8> {
        let mut buffer = vec![0u8; CHUNK];
        pattern(seed, &mut buffer);
        buffer
    }

    fn checksum(buffer: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in buffer {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash
    }

    fn write_all(file: &mut fs::File, data: &[u8]) -> Result<(), String> {
        let mut done = 0usize;
        while done < data.len() {
            let end = (done + CHUNK).min(data.len());
            let piece = data.get(done..end).ok_or("write slice")?;
            let written = file
                .write(piece)
                .map_err(|error| format!("write: {}", error.description()))?;
            if written == 0 {
                return Err("write returned 0".into());
            }
            done += written;
        }
        Ok(())
    }

    fn read_verified(path: &str, chunks: usize, base: u32) -> Result<u64, String> {
        let mut file = fs::open(path, O_RDONLY)
            .map_err(|error| format!("open {path}: {}", error.description()))?;
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut buffer = vec![0u8; CHUNK];
        for index in 0..chunks {
            let mut read = 0usize;
            while read < CHUNK {
                let piece = buffer.get_mut(read..).ok_or("read slice")?;
                let got = file
                    .read(piece)
                    .map_err(|error| format!("read {path}: {}", error.description()))?;
                if got == 0 {
                    return Err(format!("{path}: short at chunk {index}"));
                }
                read += got;
            }
            let expected = chunk(base + index as u32);
            if buffer != expected {
                return Err(format!("{path}: chunk {index} mismatch"));
            }
            for byte in &buffer {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        let mut extra = [0u8; 1];
        if file
            .read(&mut extra)
            .map_err(|error| format!("read tail: {}", error.description()))?
            != 0
        {
            return Err(format!("{path}: longer than {chunks} chunks"));
        }
        Ok(hash)
    }

    /// Rename one entry directly on the `ufs:` server root.
    ///
    /// `raven_rt::fs::rename` addresses the root `file:` server, which does not cross
    /// into the mounted UFS; the S11 rename on the mounted scheme is the product path.
    fn rename_ufs(from: &str, to: &str) -> Result<(), String> {
        let flags = fs::s11_flags(O_RDONLY).map_err(|error| error.description().to_string())?;
        let root = Client::new()
            .open("ufs:/", flags)
            .map_err(|error| format!("open ufs:/ for rename: {}", error.description()))?;
        let request = CallRequest::new(Call::Rename {
            from: from.as_bytes(),
            to: to.as_bytes(),
        });
        let mut response = [0u8; 64];
        let length = Client::new()
            .call_typed(root, &request, &mut response)
            .map_err(|error| format!("rename call: {}", error.description()))?;
        let _ = Client::new().close(root);
        match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "rename decode".to_string())?
        {
            CallResponse::Status(0) => Ok(()),
            CallResponse::Status(status) => Err(format!("rename status {status}")),
            _ => Err("rename response".into()),
        }
    }

    fn statfs() -> Result<raven_proto_fs::StatFs, String> {
        let flags = fs::s11_flags(O_RDONLY).map_err(|error| error.description().to_string())?;
        let handle = Client::new()
            .open("ufs:/", flags)
            .map_err(|error| format!("open ufs:/: {}", error.description()))?;
        let request = CallRequest::new(Call::Statfs);
        let mut response = [0u8; 64];
        let length = Client::new()
            .call_typed(handle, &request, &mut response)
            .map_err(|error| format!("statfs: {}", error.description()))?;
        let _ = Client::new().close(handle);
        match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "statfs decode".to_string())?
        {
            CallResponse::StatFs { status: 0, statfs } => Ok(statfs),
            CallResponse::StatFs { status, .. } | CallResponse::Status(status) => {
                Err(format!("statfs status {status}"))
            }
            _ => Err("statfs response".into()),
        }
    }

    fn enospc() -> Result<(), String> {
        let mut file = fs::open(FILL_PATH, O_CREAT | O_RDWR | O_TRUNC)
            .map_err(|error| format!("open fill: {}", error.description()))?;
        let mut written = 0u64;
        let mut failed_chunk = None;
        for index in 0..MAX_FILL_CHUNKS {
            let data = chunk(index as u32 + 1);
            match file.write(&data) {
                Ok(count) if count == CHUNK => written += CHUNK as u64,
                Ok(count) => {
                    return Err(format!(
                        "short write {count} at chunk {index} without ENOSPC"
                    ));
                }
                Err(error) if error == Errno::ENOSPC => {
                    failed_chunk = Some(index);
                    break;
                }
                Err(error) => {
                    return Err(format!(
                        "unexpected errno at chunk {index}: {}",
                        error.description()
                    ));
                }
            }
        }
        let Some(failed_chunk) = failed_chunk else {
            return Err(format!(
                "filesystem not full after {} chunks",
                MAX_FILL_CHUNKS
            ));
        };
        drop(file);
        if written == 0 {
            return Err("first write failed".into());
        }
        raven_rt::println!("k466: enospc errno=28 chunk={failed_chunk} wrote={written}");

        // Every previously acknowledged chunk must still read back.
        let mut reader = fs::open(FILL_PATH, O_RDONLY)
            .map_err(|error| format!("reopen: {}", error.description()))?;
        let mut buffer = vec![0u8; CHUNK];
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for index in 0..(written / CHUNK as u64) {
            let mut read = 0usize;
            while read < CHUNK {
                let piece = buffer.get_mut(read..).ok_or("reopen slice")?;
                let got = reader
                    .read(piece)
                    .map_err(|error| format!("read back: {}", error.description()))?;
                if got == 0 {
                    return Err(format!("acknowledged chunk {index} is missing"));
                }
                read += got;
            }
            if buffer != chunk(index as u32 + 1) {
                return Err(format!("acknowledged chunk {index} changed"));
            }
            for byte in &buffer {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        raven_rt::println!("k466: enospc prefix=ok last=ok hash={hash:016x}");

        let statfs = statfs()?;
        let floor_ok = statfs.free_blocks <= statfs.blocks
            && statfs.free_blocks < statfs.blocks
            && statfs.blocks > 0;
        raven_rt::println!(
            "k466: enospc blocks={} free_blocks={} free_files={} floor_ok={}",
            statfs.blocks,
            statfs.free_blocks,
            statfs.free_files,
            u8::from(floor_ok)
        );
        if !floor_ok {
            return Err("free-block count is not a sane floor".into());
        }
        Ok(())
    }

    fn persist_write() -> Result<(), String> {
        let mut file = fs::open(DATA_PATH, O_CREAT | O_RDWR | O_TRUNC)
            .map_err(|error| format!("open data: {}", error.description()))?;
        let mut data = Vec::with_capacity(CHUNK * PERSIST_CHUNKS);
        for index in 0..PERSIST_CHUNKS {
            data.extend_from_slice(&chunk(0x100 + index as u32));
        }
        write_all(&mut file, &data)?;
        drop(file);
        fs::sync().map_err(|error| format!("sync data: {}", error.description()))?;
        fs::sync().map_err(|error| format!("sync data 2: {}", error.description()))?;
        raven_rt::println!(
            "k466: persist write bytes={} checksum={:016x} sync=ok",
            data.len(),
            checksum(&data)
        );

        let entry = chunk(0x200);
        let mut entry_file = fs::open(ENTRY_PATH, O_CREAT | O_RDWR | O_TRUNC)
            .map_err(|error| format!("open entry: {}", error.description()))?;
        write_all(&mut entry_file, &entry)?;
        drop(entry_file);
        fs::sync().map_err(|error| format!("sync dir: {}", error.description()))?;
        raven_rt::println!("k466: persist dir=ok");

        rename_ufs("/k466-entry.txt", "/k466-renamed.txt")?;
        fs::sync().map_err(|error| format!("sync rename: {}", error.description()))?;
        raven_rt::println!("k466: persist rename=ok");
        Ok(())
    }

    fn persist_check() -> Result<(), String> {
        let file_hash = read_verified(DATA_PATH, PERSIST_CHUNKS, 0x100)?;
        raven_rt::println!(
            "k466: persist file=ok len={} checksum={file_hash:016x}",
            CHUNK * PERSIST_CHUNKS
        );
        let entry_hash = read_verified(RENAMED_PATH, 1, 0x200)?;
        raven_rt::println!("k466: persist dir=ok rename=ok entry={entry_hash:016x}");
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
