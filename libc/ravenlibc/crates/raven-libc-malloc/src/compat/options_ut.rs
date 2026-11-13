// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn defaults_match_the_donor() {
    let options = Options::default();
    assert_eq!(options.malloc_mutexes, 8);
    assert_eq!(options.def_malloc_junk, 1);
    assert_eq!(options.def_maxcache, 64);
    assert!(!options.chunk_canaries);
}

#[test]
fn pool_letters_clamp_at_two_and_thirty_two() {
    let mut options = Options::default();
    for _ in 0..10 {
        assert!(options.apply(b'+'));
    }
    assert_eq!(options.malloc_mutexes, 32);
    for _ in 0..10 {
        assert!(options.apply(b'-'));
    }
    assert_eq!(options.malloc_mutexes, 2);
}

#[test]
fn cache_letters_clamp_at_zero_and_256() {
    let mut options = Options::default();
    for _ in 0..10 {
        assert!(options.apply(b'>'));
    }
    assert_eq!(options.def_maxcache, 256);
    for _ in 0..10 {
        assert!(options.apply(b'<'));
    }
    assert_eq!(options.def_maxcache, 0);
}

#[test]
fn canary_freecheck_guard_and_realloc_letters() {
    let mut options = Options::default();
    assert!(options.apply(b'C'));
    assert!(options.chunk_canaries);
    assert!(options.apply(b'c'));
    assert!(!options.chunk_canaries);
    assert!(options.apply(b'F'));
    assert!(options.malloc_freecheck && options.malloc_freeunmap);
    assert!(options.apply(b'f'));
    assert!(!options.malloc_freecheck && !options.malloc_freeunmap);
    assert!(options.apply(b'G'));
    assert_eq!(options.malloc_guard, 4096);
    assert!(options.apply(b'g'));
    assert_eq!(options.malloc_guard, 0);
    assert!(options.apply(b'R'));
    assert!(options.malloc_realloc);
    assert!(options.apply(b'r'));
    assert!(!options.malloc_realloc);
}

#[test]
fn junk_levels_clamp() {
    let mut options = Options::default();
    assert!(options.apply(b'J'));
    assert_eq!(options.def_malloc_junk, 2);
    assert!(options.apply(b'J'));
    assert_eq!(options.def_malloc_junk, 2);
    assert!(options.apply(b'j'));
    assert_eq!(options.def_malloc_junk, 1);
    assert!(options.apply(b'j'));
    assert_eq!(options.def_malloc_junk, 0);
}

#[test]
fn security_set_applies_and_clears() {
    let mut options = Options::default();
    assert_eq!(options.parse(b"S"), None);
    assert!(options.chunk_canaries);
    assert!(options.malloc_freecheck);
    assert!(options.malloc_freeunmap);
    assert!(options.malloc_guard != 0);
    assert_eq!(options.def_maxcache, 0);
    assert_eq!(options.parse(b"s"), None);
    assert!(!options.chunk_canaries);
    assert_eq!(options.def_maxcache, 64);
}

#[test]
fn unknown_letters_are_reported_once() {
    let mut options = Options::default();
    assert_eq!(options.parse(b"Q"), Some(b'Q'));
    assert_eq!(options.parse(b"xX"), None);
}

#[test]
fn unmap_and_freecheck_letters_are_independent() {
    let mut options = Options::default();
    assert!(options.apply(b'U'));
    assert!(options.malloc_freeunmap);
    assert!(options.apply(b'u'));
    assert!(!options.malloc_freeunmap);
}

#[test]
fn xmalloc_letters() {
    let mut options = Options::default();
    assert!(options.apply(b'X'));
    assert!(options.malloc_xmalloc);
    assert!(options.apply(b'x'));
    assert!(!options.malloc_xmalloc);
}

#[test]
fn stats_letters_are_accepted_and_ignored() {
    let mut options = Options::default();
    assert!(options.apply(b'D'));
    assert!(options.malloc_stats);
    assert!(options.apply(b'd'));
    assert!(!options.malloc_stats);
}

#[test]
fn verbose_letters_are_accepted() {
    let mut options = Options::default();
    assert!(options.apply(b'V'));
    assert!(options.malloc_verbose);
    assert!(options.apply(b'v'));
    assert!(!options.malloc_verbose);
}

#[test]
fn pool_clamp_never_drops_below_two() {
    let mut options = Options {
        malloc_mutexes: 2,
        ..Options::default()
    };
    assert!(options.apply(b'-'));
    assert_eq!(options.malloc_mutexes, 2);
}
