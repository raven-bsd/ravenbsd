// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The stdio `Env`: `raven-libc-stdio`'s [`SysFile`] and [`Lock`] seams over
//! `raven-libc-platform` (C56 module documentation says the shell wires the two).

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use raven_libc_abi::errno::Errno;
use raven_libc_stdio::traits::{Lock, StatLite, SysFile};

use super::platform;

/// The shell's stdio environment.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellEnv;

fn error(e: raven_libc_platform::Errno) -> Errno {
    Errno(e.0)
}

impl SysFile for ShellEnv {
    fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        platform::global().read(fd, buf).map_err(error)
    }

    fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno> {
        platform::global().write(fd, buf).map_err(error)
    }

    fn write2(&self, fd: i32, a: &[u8], b: &[u8]) -> Result<usize, Errno> {
        match platform::global().writev(fd, &[a, b]) {
            Ok(count) => Ok(count),
            Err(raven_libc_platform::Errno::EINVAL) => {
                // A scheme server may refuse a two-part request; fall back to two writes.
                let first = platform::global().write(fd, a).map_err(error)?;
                if first == a.len() {
                    let second = platform::global().write(fd, b).map_err(error)?;
                    Ok(first.saturating_add(second))
                } else {
                    Ok(first)
                }
            }
            Err(other) => Err(error(other)),
        }
    }

    fn seek(&self, fd: i32, off: i64, whence: i32) -> Result<i64, Errno> {
        platform::global().lseek(fd, off, whence).map_err(error)
    }

    fn close(&self, fd: i32) -> Result<(), Errno> {
        platform::global().close(fd).map_err(error)
    }

    fn open(&self, path: &[u8], flags: i32, mode: u32) -> Result<i32, Errno> {
        platform::global()
            .open(path, flags as u32, mode)
            .map_err(error)
    }

    fn isatty(&self, fd: i32) -> bool {
        raven_libc_platform::tty::isatty(platform::global(), fd)
    }

    fn fstat(&self, fd: i32) -> Result<StatLite, Errno> {
        let stat = platform::global().fstat(fd).map_err(error)?;
        Ok(StatLite {
            mode: stat.st_mode,
            blksize: u32::try_from(stat.st_blksize).unwrap_or(0),
            is_char_dev: stat.st_mode & raven_libc_abi::stat::S_IFMT
                == raven_libc_abi::stat::S_IFCHR,
        })
    }

    fn getfl(&self, fd: i32) -> Result<i32, Errno> {
        let value = platform::global()
            .fcntl(fd, raven_libc_abi::fcntl::F_GETFL, 0)
            .map_err(error)?;
        i32::try_from(value).map_err(|_| Errno(22))
    }

    fn setfl(&self, fd: i32, flags: i32) -> Result<(), Errno> {
        platform::global()
            .fcntl(fd, raven_libc_abi::fcntl::F_SETFL, i64::from(flags))
            .map_err(error)?;
        Ok(())
    }

    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        platform::global().unlink(path).map_err(error)
    }

    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        platform::global().rmdir(path).map_err(error)
    }

    fn rename(&self, old: &[u8], new: &[u8]) -> Result<(), Errno> {
        platform::global().rename(old, new).map_err(error)
    }

    fn pipe2(&self, flags: i32) -> Result<(i32, i32), Errno> {
        platform::global().pipe2(flags).map_err(error)
    }

    fn random_suffix(&self, out: &mut [u8; 6]) {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let value = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut state = value ^ 0x9e37_79b9;
        for slot in out.iter_mut() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = state % 36;
            *slot = b"abcdefghijklmnopqrstuvwxyz0123456789"
                .get(index)
                .copied()
                .unwrap_or(b'0');
        }
    }
}

impl Lock for ShellEnv {
    fn tid(&self) -> i32 {
        1
    }

    fn wait(&self, word: &AtomicI32, expected: i32) {
        while word.load(Ordering::Acquire) == expected {
            core::hint::spin_loop();
        }
    }

    fn wake(&self, _word: &AtomicI32, _n: i32) {}

    fn threaded(&self) -> bool {
        false
    }
}

/// The one stdio environment value.
pub(crate) const ENV: ShellEnv = ShellEnv;
