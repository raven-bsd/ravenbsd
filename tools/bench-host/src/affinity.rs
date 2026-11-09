// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pin the measurement process to one CPU and restore the previous affinity.

use std::fs;

use anyhow::{Result, anyhow};

const CPU_WORDS: usize = 16;

pub(crate) struct AffinityGuard {
    original: [u64; CPU_WORDS],
}

impl Drop for AffinityGuard {
    fn drop(&mut self) {
        // SAFETY: `original` is a kernel cpu_set_t-sized buffer owned by this guard, and the
        // syscall only reads its documented byte size.
        let _ = unsafe {
            libc::syscall(
                libc::SYS_sched_setaffinity,
                0,
                std::mem::size_of::<[u64; CPU_WORDS]>(),
                self.original.as_ptr(),
            )
        };
    }
}

pub(crate) fn pin_first_cpu() -> Result<AffinityGuard> {
    let size = std::mem::size_of::<[u64; CPU_WORDS]>();
    let mut original = [0u64; CPU_WORDS];
    // SAFETY: `original` is a writable 128-byte buffer and `size` describes it; the kernel
    // writes at most the affinity mask size.
    let status =
        unsafe { libc::syscall(libc::SYS_sched_getaffinity, 0, size, original.as_mut_ptr()) };
    if status < 0 {
        return Err(anyhow!(
            "sched_getaffinity failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let cpu = first_allowed_cpu().unwrap_or(0);
    let mut mask = [0u64; CPU_WORDS];
    if cpu / 64 >= CPU_WORDS {
        return Err(anyhow!("CPU {cpu} exceeds the cpu_set_t size"));
    }
    mask[cpu / 64] |= 1u64 << (cpu % 64);
    // SAFETY: `mask` is a readable 128-byte cpu_set_t with exactly one CPU bit set; the kernel
    // only reads the documented affinity size.
    let status = unsafe { libc::syscall(libc::SYS_sched_setaffinity, 0, size, mask.as_ptr()) };
    if status < 0 {
        return Err(anyhow!(
            "sched_setaffinity failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(AffinityGuard { original })
}

fn first_allowed_cpu() -> Option<usize> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))?
        .trim();
    let first = list.split(',').next()?.split('-').next()?;
    first.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_first_cpu() {
        // The status file always exists on the Linux test host.
        let cpu = first_allowed_cpu().expect("Cpus_allowed_list");
        assert!(cpu < 1024);
    }
}
