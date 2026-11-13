// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Model-based randomized test over both backends (doc 08 7.2).

#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap, Tiny};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() as usize) % n
        }
    }
}

struct Live {
    address: usize,
    size: usize,
    pattern: u8,
}

fn operations() -> usize {
    std::env::var("RL_MODEL_OPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000_000)
}

fn check_live(heap: &dyn Heap, live: &[Live]) {
    for block in live {
        let mut index = 0usize;
        while index < block.size {
            let value = unsafe { *(block.address as *const u8).add(index) };
            assert_eq!(
                value, block.pattern,
                "block {:#x} offset {index}",
                block.address
            );
            index += 1;
        }
        assert!(heap.usable_size(block.address) >= block.size);
    }
}

fn overlaps(live: &[Live], address: usize, size: usize) -> bool {
    live.iter().any(|block| {
        address < block.address.saturating_add(block.size.max(1))
            && block.address < address.saturating_add(size.max(1))
    })
}

fn run_hardened(seed: u64, ops: usize) {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(seed),
        TestPlatform::new(1),
        b"",
    );
    let mut rng = Rng(seed | 1);
    let mut live: Vec<Live> = Vec::new();
    let sizes = [
        0usize,
        1,
        15,
        16,
        17,
        2047,
        2048,
        2049,
        4095,
        4096,
        4097,
        65_535,
        65_536,
        1 << 20,
    ];
    let aligns = [1usize, 2, 8, 16, 64, 4096, 8192, 65_536];
    let mut step = 0usize;
    while step < ops {
        let choice = rng.below(100);
        if choice < 35 || live.is_empty() {
            let size = sizes[rng.below(sizes.len())];
            let address = heap.malloc(size);
            if address != 0 {
                assert_eq!(address % 16, 0);
                assert!(heap.usable_size(address) >= size);
                assert!(!overlaps(&live, address, size));
                let pattern = (step as u8).wrapping_add(1);
                if size != 0 {
                    unsafe { core::ptr::write_bytes(address as *mut u8, pattern, size) };
                }
                live.push(Live {
                    address,
                    size,
                    pattern,
                });
            }
        } else if choice < 45 {
            let n = rng.below(32);
            let m = rng.below(32);
            let address = heap.calloc(n, m);
            if address != 0 {
                let size = n * m;
                let mut index = 0usize;
                while index < size {
                    assert_eq!(unsafe { *(address as *const u8).add(index) }, 0);
                    index += 1;
                }
                live.push(Live {
                    address,
                    size,
                    pattern: 0,
                });
            }
        } else if choice < 65 {
            let index = rng.below(live.len());
            let new_size = sizes[rng.below(sizes.len())];
            if new_size == 0 {
                let block = live.remove(index);
                heap.free(block.address);
                step += 1;
                continue;
            }
            let old = live[index].address;
            let old_size = live[index].size;
            let old_pattern = live[index].pattern;
            let new = heap.realloc(old, new_size);
            if new != 0 {
                let copy = old_size.min(new_size);
                let mut offset = 0usize;
                while offset < copy {
                    assert_eq!(
                        unsafe { *(new as *const u8).add(offset) },
                        old_pattern,
                        "realloc lost content"
                    );
                    offset += 1;
                }
                assert_eq!(new % 16, 0);
                assert!(heap.usable_size(new) >= new_size);
                live.remove(index);
                assert!(!overlaps(&live, new, new_size));
                if new_size > old_size {
                    unsafe {
                        core::ptr::write_bytes(
                            (new + old_size) as *mut u8,
                            old_pattern,
                            new_size - old_size,
                        )
                    };
                }
                live.push(Live {
                    address: new,
                    size: new_size,
                    pattern: old_pattern,
                });
            }
        } else if choice < 70 {
            let align = aligns[rng.below(aligns.len())];
            let size = sizes[rng.below(sizes.len())];
            let address = heap.memalign(align, size);
            if address != 0 {
                assert_eq!(address % align, 0);
                assert!(heap.usable_size(address) >= size);
                assert!(!overlaps(&live, address, size));
                if size != 0 {
                    unsafe { core::ptr::write_bytes(address as *mut u8, 0x5a, size) };
                }
                live.push(Live {
                    address,
                    size,
                    pattern: 0x5a,
                });
            }
        } else {
            let index = rng.below(live.len());
            let block = live.remove(index);
            check_live(&heap, core::slice::from_ref(&block));
            heap.free(block.address);
        }
        if step.is_multiple_of(10_000) {
            check_live(&heap, &live);
        }
        step += 1;
    }
    for block in live {
        heap.free(block.address);
    }
}

fn run_tiny(seed: u64, ops: usize) {
    let heap = Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1));
    let mut rng = Rng(seed | 1);
    let mut live: Vec<Live> = Vec::new();
    let sizes = [
        0usize, 1, 15, 16, 17, 2047, 2048, 2049, 4095, 4096, 4097, 65_535,
    ];
    let mut step = 0usize;
    while step < ops {
        let choice = rng.below(100);
        if choice < 40 || live.is_empty() {
            let size = sizes[rng.below(sizes.len())];
            let address = heap.malloc(size);
            if address != 0 {
                assert_eq!(address % 16, 0);
                assert!(heap.usable_size(address) >= size);
                assert!(!overlaps(&live, address, size));
                let pattern = (step as u8).wrapping_add(1);
                if size != 0 {
                    unsafe { core::ptr::write_bytes(address as *mut u8, pattern, size) };
                }
                live.push(Live {
                    address,
                    size,
                    pattern,
                });
            }
        } else if choice < 55 {
            let index = rng.below(live.len());
            let new_size = sizes[rng.below(sizes.len())];
            if new_size == 0 {
                let block = live.remove(index);
                heap.free(block.address);
                step += 1;
                continue;
            }
            let old = live[index].address;
            let old_size = live[index].size;
            let old_pattern = live[index].pattern;
            let new = heap.realloc(old, new_size);
            if new != 0 {
                let copy = old_size.min(new_size);
                let mut offset = 0usize;
                while offset < copy {
                    assert_eq!(unsafe { *(new as *const u8).add(offset) }, old_pattern);
                    offset += 1;
                }
                live.remove(index);
                assert!(!overlaps(&live, new, new_size));
                if new_size > old_size {
                    unsafe {
                        core::ptr::write_bytes(
                            (new + old_size) as *mut u8,
                            old_pattern,
                            new_size - old_size,
                        )
                    };
                }
                live.push(Live {
                    address: new,
                    size: new_size,
                    pattern: old_pattern,
                });
            }
        } else {
            let index = rng.below(live.len());
            let block = live.remove(index);
            check_live(&heap, core::slice::from_ref(&block));
            heap.free(block.address);
        }
        if step.is_multiple_of(10_000) {
            check_live(&heap, &live);
        }
        step += 1;
    }
    for block in live {
        heap.free(block.address);
    }
}

#[test]
fn hardened_model() {
    let ops = operations();
    for seed in [1u64, 2, 3, 0xdead_c0de, 0x5eed] {
        run_hardened(seed, ops);
    }
}

#[test]
fn tiny_model() {
    let ops = operations();
    for seed in [1u64, 2, 3] {
        run_tiny(seed, ops);
    }
}
