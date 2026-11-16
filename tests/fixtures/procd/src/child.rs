// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2e procd child: prints the argv/environment handed to it, then exits or sleeps.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The SIGRECV wrapper below has a SAFETY comment.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_rt::entry;

    const PROC_SIGRECV: usize = 0x4B;
    const PROC_SELF_INFO: usize = 0x35;
    const FDMAP_MAGIC: &[u8; 4] = b"RFDM";
    const FDMAP_HEADER: usize = 8;
    const FDMAP_ENTRY: usize = 18;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mode = args.get(1).unwrap_or("missing");
        match mode {
            "exec" => {
                let test = args.env_get("TEST").unwrap_or("<unset>");
                raven_rt::println!("procd-child: exec TEST={test}");
                7
            }
            "sleep" => {
                raven_rt::println!("procd-child: sleeping");
                loop {
                    let signal = receive_signal();
                    if signal == 15 {
                        raven_rt::println!("procd-child: got signal 15");
                        return 143;
                    }
                    if signal < 0 {
                        let _ = raven_rt::nanosleep(10_000_000);
                    }
                }
            }
            "attrs" => attrs(&args),
            _ => {
                raven_rt::println!("procd-child: unknown mode={mode}");
                2
            }
        }
    }

    fn attrs(args: &raven_rt::Args) -> i32 {
        let expected_pgid = args
            .get(2)
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(-1);
        let expect_signals = args.get(3) == Some("1");
        let (fd_ok, cloexec_ok) = inherited_fds(args);
        let pgid_ok = shared_pgid(expected_pgid);
        let sig_ok = if expect_signals {
            signal_present(args, raven_rt::spawn::AT_RAVEN_SIGMASK, 10)
                && signal_present(args, raven_rt::spawn::AT_RAVEN_SIGDEFAULT, 15)
                && signal_present(args, raven_rt::spawn::AT_RAVEN_SIGIGNORE, 12)
        } else {
            true
        };
        raven_rt::println!(
            "procd-child: attrs fd={} cloexec={} pgid={} sig={}",
            u8::from(fd_ok),
            u8::from(cloexec_ok),
            u8::from(pgid_ok),
            u8::from(sig_ok)
        );
        if fd_ok && cloexec_ok && pgid_ok && sig_ok {
            raven_rt::println!("procd-child: ok");
            // Keep the child alive long enough for procd's post-spawn pid/group queries.
            let _ = raven_rt::nanosleep(100_000_000);
            0
        } else {
            3
        }
    }

    fn inherited_fds(args: &raven_rt::Args) -> (bool, bool) {
        let pointer = args.auxv(raven_rt::spawn::AT_RAVEN_FDMAP).unwrap_or(0);
        let count = args.auxv(raven_rt::spawn::AT_RAVEN_FDMAP_N).unwrap_or(0);
        if pointer == 0 || count == 0 || count > 256 {
            return (false, false);
        }
        let len = FDMAP_HEADER.saturating_add(FDMAP_ENTRY.saturating_mul(count));
        // SAFETY: the spawner mapped exactly this header+entry block at `pointer` before the
        // first thread started; `count` is its decoded entry count and the block has no strings.
        let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, len) };
        if bytes.get(0..4) != Some(FDMAP_MAGIC.as_slice()) {
            return (false, false);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let entries = u16::from_le_bytes([bytes[6], bytes[7]]);
        if version != 1 || usize::from(entries) != count {
            return (false, false);
        }
        let mut handle = None;
        let mut extra = false;
        for index in 0..count {
            let base = FDMAP_HEADER.saturating_add(FDMAP_ENTRY.saturating_mul(index));
            let Some(record) = bytes.get(base..base.saturating_add(FDMAP_ENTRY)) else {
                return (false, false);
            };
            let fd = i32::from_le_bytes([record[0], record[1], record[2], record[3]]);
            let value = u32::from_le_bytes([record[4], record[5], record[6], record[7]]);
            if fd == 7 {
                handle = Some(value);
            } else if fd == 8 {
                extra = true;
            }
        }
        let Some(handle) = handle else {
            return (false, false);
        };
        let mut buffer = [0u8; 4];
        let Ok(read) = raven_rt::sys::read(Handle(handle), &mut buffer) else {
            return (false, false);
        };
        (&buffer[..read] == b"ok\n", !extra)
    }

    fn signal_present(args: &raven_rt::Args, key: usize, signo: u32) -> bool {
        let pointer = args.auxv(key).unwrap_or(0);
        if pointer == 0 {
            return false;
        }
        // SAFETY: the spawner mapped a 128-byte signal set at `pointer` before the first thread.
        let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, 128) };
        let bit = usize::try_from(signo.saturating_sub(1)).unwrap_or(usize::MAX);
        bit < 1024 && bytes[bit / 8] & (1 << (bit % 8)) != 0
    }

    fn self_ids() -> Option<(i32, i32, i32, i32)> {
        let mut words = [0u64; 8];
        // SAFETY: PROC_SELF_INFO writes eight u64 words to the live array owned by the caller.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len())
        };
        Errno::from_ret(raw).ok()?;
        Some((
            i32::try_from(words[0]).ok()?,
            i32::try_from(words[1]).ok()?,
            i32::try_from(words[2]).ok()?,
            i32::try_from(words[3]).ok()?,
        ))
    }

    /// procd applies `SETPGROUP` after the first thread exists, so poll briefly for the final id.
    fn shared_pgid(expected: i32) -> bool {
        for _ in 0..100 {
            if let Some((pid, _, pgid, _)) = self_ids() {
                let matched = if expected < 0 {
                    pid > 0 && pgid == pid
                } else {
                    pgid == expected
                };
                if matched {
                    return true;
                }
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
        false
    }

    fn receive_signal() -> i32 {
        let mut words = [0u64; 3];
        // SAFETY: SIGRECV writes three u64 words to the live array owned by the caller; a
        // blocking zero flags call waits for the next queued signal.
        let raw = unsafe { raven_rt::sys::syscall2(PROC_SIGRECV, words.as_mut_ptr() as usize, 0) };
        Errno::from_ret(raw).map_or(-1, |signo| signo as i32)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
