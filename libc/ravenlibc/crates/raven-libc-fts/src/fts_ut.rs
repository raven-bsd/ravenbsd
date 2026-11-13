// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the `fts`, `ftw` and `nftw` walk over the fake tree.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;

use raven_libc_abi::generated::errno::{EACCES, EINVAL, EIO, ELOOP, ENAMETOOLONG, ENOENT};
use raven_libc_abi::generated::fcntl::O_NOFOLLOW;

use super::*;
use crate::fake::{FakeFs, NodeKind};
use crate::{
    AT_FDCWD, DirSys, FTW_CHDIR, FTW_D, FTW_DEPTH, FTW_DP, FTW_F, FTW_MOUNT, FTW_SLN, Fts, Ftw,
    ftw, nftw,
};

struct Walked {
    info: i32,
    path: Vec<u8>,
    name: Vec<u8>,
    level: i64,
    errno: i32,
    cycle: Option<NodeId>,
    statp: bool,
}

fn drain(fts: &mut Fts, fs: &mut FakeFs) -> Vec<Walked> {
    let mut out = Vec::new();
    while let Some(node) = fts.read(fs).unwrap() {
        out.push(Walked {
            info: node.info,
            path: node.path.clone(),
            name: node.name.clone(),
            level: node.level,
            errno: node.errno,
            cycle: node.cycle,
            statp: node.statp.is_some(),
        });
    }
    out
}

fn tree() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.file("r/a", 1);
    fs.mkdir("r/sub", 0o755);
    fs.file("r/sub/x", 2);
    fs
}

fn reverse(a: &FtsEnt, b: &FtsEnt) -> i32 {
    match b.name.cmp(&a.name) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[test]
fn preorder_and_postorder() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let infos: Vec<i32> = nodes.iter().map(|n| n.info).collect();
    let paths: Vec<&[u8]> = nodes.iter().map(|n| n.path.as_slice()).collect();
    assert_eq!(
        paths,
        vec![
            b"r".as_slice(),
            b"r/a",
            b"r/sub",
            b"r/sub/x",
            b"r/sub",
            b"r"
        ]
    );
    assert_eq!(infos, vec![FTS_D, FTS_F, FTS_D, FTS_F, FTS_DP, FTS_DP]);
    assert_eq!(nodes[0].level, 0);
    assert_eq!(nodes[3].level, 2);
}

#[test]
fn no_chdir_is_forced_and_visible() {
    let mut fs = tree();
    let fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], 0, None).unwrap();
    assert_ne!(fts.options() & FTS_NOCHDIR, 0);
    fts.close(&mut fs).unwrap();
}

#[test]
fn physical_walk_does_not_follow_symlinks() {
    let mut fs = tree();
    fs.symlink("r/link", "sub");
    fs.symlink("r/link2", "a");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let link = nodes.iter().find(|n| n.name == b"link").unwrap();
    assert_eq!(link.info, FTS_SL);
    let link2 = nodes.iter().find(|n| n.name == b"link2").unwrap();
    assert_eq!(link2.info, FTS_SL);
    assert!(!nodes.iter().any(|n| n.path == b"r/link/x"));
}

#[test]
fn logical_walk_follows_symlinks() {
    let mut fs = tree();
    fs.symlink("r/link", "sub");
    fs.symlink("r/link2", "a");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_LOGICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let link = nodes.iter().find(|n| n.name == b"link").unwrap();
    assert_eq!(link.info, FTS_D);
    assert!(nodes.iter().any(|n| n.path == b"r/link/x"));
    let link2 = nodes.iter().find(|n| n.name == b"link2").unwrap();
    assert_eq!(link2.info, FTS_F);
}

#[test]
fn a_symlink_cycle_is_reported_once_and_not_entered() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.mkdir("r/a", 0o755);
    fs.file("r/a/f", 1);
    fs.symlink("r/a/b", "..");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_LOGICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let cycle = nodes.iter().find(|n| n.path == b"r/a/b").unwrap();
    assert_eq!(cycle.info, FTS_DC);
    assert!(cycle.cycle.is_some());
    let a_visits = nodes
        .iter()
        .filter(|n| n.path == b"r/a" && n.info == FTS_D)
        .count();
    assert_eq!(a_visits, 1);
}

#[test]
fn fts_skip_turns_a_directory_into_postorder() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _root = fts.read(&mut fs).unwrap().unwrap().id;
    let _a = fts.read(&mut fs).unwrap().unwrap().id;
    let sub = fts.read(&mut fs).unwrap().unwrap().id;
    fts.set(sub, FTS_SKIP).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_DP);
    assert_eq!(node.path, b"r/sub");
    assert_eq!(node.id, sub);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_again_revisits_a_node() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _root = fts.read(&mut fs).unwrap().unwrap().id;
    let a = fts.read(&mut fs).unwrap().unwrap().id;
    fts.set(a, FTS_AGAIN).unwrap();
    let again = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(again.id, a);
    assert_eq!(again.info, FTS_F);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_follow_revisits_a_symlink() {
    let mut fs = tree();
    fs.symlink("r/link", "sub");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _root = fts.read(&mut fs).unwrap().unwrap().id;
    let link = loop {
        let node = fts.read(&mut fs).unwrap().unwrap();
        if node.name == b"link" {
            break node.id;
        }
    };
    fts.set(link, FTS_FOLLOW).unwrap();
    let followed = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(followed.id, link);
    assert_eq!(followed.info, FTS_D);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_xdev_stops_at_a_mount() {
    let mut fs = tree();
    fs.mkdir("r/mnt", 0o755);
    fs.file("r/mnt/inner", 1);
    fs.mount("r/mnt", 2);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL | FTS_XDEV, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(!nodes.iter().any(|n| n.path == b"r/mnt/inner"));
    let mnt = nodes.iter().rev().find(|n| n.path == b"r/mnt").unwrap();
    assert_eq!(mnt.info, FTS_DP);
}

#[test]
fn an_unreadable_directory_is_dnr() {
    let mut fs = tree();
    fs.mkdir("r/locked", 0o000);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let locked = nodes.iter().rev().find(|n| n.name == b"locked").unwrap();
    assert_eq!(locked.info, FTS_DNR);
    assert_eq!(locked.errno, EACCES);
}

#[test]
fn children_nameonly_returns_names_without_stat() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _root = fts.read(&mut fs).unwrap().unwrap().id;
    let names = fts.children(&mut fs, FTS_NAMEONLY as i32).unwrap();
    let names: Vec<(Vec<u8>, i32)> = names.iter().map(|n| (n.name.clone(), n.info)).collect();
    assert_eq!(names.len(), 2);
    assert!(names.iter().all(|(_, info)| *info == FTS_NSOK));
    fts.close(&mut fs).unwrap();
}

#[test]
fn children_before_the_first_read_are_the_roots() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r", b"r/a"], FTS_PHYSICAL, None).unwrap();
    let roots = fts.children(&mut fs, 0).unwrap();
    let paths: Vec<Vec<u8>> = roots.iter().map(|n| n.path.clone()).collect();
    assert_eq!(paths, vec![b"r".to_vec(), b"r/a".to_vec()]);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_openat_walks_relative_to_a_directory_descriptor() {
    let mut fs = FakeFs::new();
    fs.mkdir("base", 0o755);
    fs.mkdir("base/d", 0o755);
    fs.file("base/d/x", 1);
    let fd = fs.openat(AT_FDCWD, b"base", 0).unwrap();
    let mut fts = Fts::open(&mut fs, fd.0, &[b"d"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    fs.close(fd).unwrap();
    assert_eq!(nodes[0].path, b"d");
    assert!(nodes.iter().any(|n| n.path == b"d/x"));
}

#[test]
fn empty_argv_and_nameonly_are_rejected() {
    let mut fs = tree();
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[], FTS_PHYSICAL, None)
            .err()
            .map(|error| error.0),
        Some(EINVAL as u16)
    );
    assert_eq!(
        Fts::open(
            &mut fs,
            AT_FDCWD,
            &[b"r"],
            FTS_PHYSICAL | FTS_NAMEONLY,
            None
        )
        .err()
        .map(|error| error.0),
        Some(EINVAL as u16)
    );
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[b"r"], 0x8000_0000, None)
            .err()
            .map(|error| error.0),
        Some(EINVAL as u16)
    );
}

#[test]
fn a_comparator_sorts_children_and_roots() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.mkdir("r/b", 0o755);
    fs.mkdir("r/a", 0o755);
    fs.file("r/c", 1);
    fs.mkdir("z", 0o755);
    fs.mkdir("y", 0o755);
    let compar: Compar = Box::new(reverse);
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"r", b"z", b"y"],
        FTS_PHYSICAL,
        Some(compar),
    )
    .unwrap();
    let roots: Vec<Vec<u8>> = {
        let list = fts.children(&mut fs, 0).unwrap();
        list.iter().map(|n| n.path.clone()).collect()
    };
    assert_eq!(roots, vec![b"z".to_vec(), b"y".to_vec(), b"r".to_vec()]);
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let children: Vec<&[u8]> = nodes
        .iter()
        .filter(|n| n.level == 1 && n.info != FTS_DP)
        .map(|n| n.name.as_slice())
        .collect();
    assert_eq!(children, vec![b"c".as_slice(), b"b", b"a"]);
}

#[test]
fn an_inconsistent_comparator_completes_the_walk() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    for i in 0..64 {
        fs.file(format!("r/f{i}"), 1);
    }
    let mut counter = 0u32;
    let compar: Compar = Box::new(move |_a: &FtsEnt, _b: &FtsEnt| {
        counter = counter.wrapping_mul(1103515245).wrapping_add(12345);
        match counter % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        }
    });
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, Some(compar)).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    // The root pre-order, the 64 files and the root post-order.
    assert_eq!(nodes.len(), 66);
}

#[test]
fn a_path_over_path_max_is_fs_err_enametoolong() {
    let mut fs = FakeFs::new();
    let mut path = alloc::string::String::new();
    for _ in 0..2200 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str("ab");
        fs.mkdir(&path, 0o755);
    }
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"ab"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(nodes.iter().all(|n| n.path.len() <= PATH_MAX));
    let long = nodes
        .iter()
        .find(|n| n.info == FTS_ERR)
        .expect("an over-long node");
    assert_eq!(long.errno, ENAMETOOLONG);
}

#[test]
fn a_deep_tree_walks_with_a_sixty_four_fd_budget() {
    // A path is capped at PATH_MAX (H-FTS-5), so the deepest legal chain with one-character
    // names is 2047 levels; 2000 levels is what the fd-budget test uses.
    let mut fs = FakeFs::new();
    let mut path = alloc::string::String::new();
    for _ in 0..2000 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push('d');
        fs.mkdir(&path, 0o755);
    }
    fs.emfile_after(64);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"d"], FTS_PHYSICAL, None).unwrap();
    let mut count = 0usize;
    let mut last_info = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        count += 1;
        last_info = node.info;
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(count, 2000 * 2);
    assert_eq!(last_info, FTS_DP);
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn a_swapped_directory_is_error_enoent_and_never_leaves_the_tree() {
    let mut fs = tree();
    fs.mkdir("r/d", 0o755);
    fs.file("r/d/x", 1);
    fs.mkdir("outside", 0o755);
    fs.file("outside/secret", 1);
    {
        let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
        let _ = drain(&mut fts, &mut fs);
        fts.close(&mut fs).unwrap();
    }
    let openat_index = fs
        .log
        .iter()
        .position(|call| call.method == "openat" && call.name == b"d")
        .map(|index| index + 1)
        .expect("the openat of d");

    let mut fs = tree();
    fs.mkdir("r/d", 0o755);
    fs.file("r/d/x", 1);
    fs.mkdir("outside", 0o755);
    fs.file("outside/secret", 1);
    fs.hook_after(
        openat_index,
        Box::new(|fs: &mut FakeFs| {
            fs.remove("r/d");
            fs.rename("outside", "r/d");
        }),
    );
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();

    let swapped = nodes.iter().find(|n| n.name == b"d").expect("the node");
    assert_eq!(swapped.info, FTS_ERR);
    assert_eq!(swapped.errno, ENOENT);
    assert!(!nodes.iter().any(|n| n.path.ends_with(b"secret")));
    for call in &fs.log {
        if call.method == "openat" {
            assert_ne!(call.flags & O_NOFOLLOW as u32, 0, "{call:?}");
        }
    }
}

#[test]
fn a_255_byte_name_walks() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    let name = "n".repeat(255);
    fs.file(format!("r/{name}"), 1);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(nodes.iter().any(|n| n.name.as_slice() == name.as_bytes()));
}

#[test]
fn nostat_skips_the_stat_buffer_and_nostat_type_uses_d_type() {
    let mut fs = tree();
    fs.symlink("r/link", "a");
    fs.node("r/fifo", NodeKind::Fifo);
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"r"],
        FTS_PHYSICAL | FTS_NOSTAT_TYPE,
        None,
    )
    .unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(nodes.iter().all(|n| !n.statp));
    let fifo = nodes.iter().find(|n| n.name == b"fifo").unwrap();
    assert_eq!(fifo.info, FTS_DEFAULT);
    let link = nodes.iter().find(|n| n.name == b"link").unwrap();
    assert_eq!(link.info, FTS_SL);
    let file = nodes.iter().find(|n| n.name == b"a").unwrap();
    assert_eq!(file.info, FTS_F);
}

#[test]
fn seedot_returns_dot_and_dotdot() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL | FTS_SEEDOT, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(nodes.iter().any(|n| n.name == b"." && n.info == FTS_DOT));
    assert!(nodes.iter().any(|n| n.name == b".." && n.info == FTS_DOT));
}

#[test]
fn fts_set_rejects_an_unknown_instruction() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let root = fts.read(&mut fs).unwrap().unwrap().id;
    assert_eq!(fts.set(root, 99).unwrap_err().0, EINVAL as u16);
    fts.close(&mut fs).unwrap();
}

#[test]
fn close_releases_every_descriptor() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    assert!(fs.open_fd_count() > 0);
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn hard_links_report_the_same_inode_and_a_shared_nlink() {
    let mut fs = FakeFs::new();
    fs.file("a", 1);
    fs.link("b", "a");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"a"], FTS_PHYSICAL, None).unwrap();
    let first = fts.read(&mut fs).unwrap().unwrap();
    let first_ino = first.ino;
    let first_nlink = first.statp.map_or(0, |stat| stat.st_nlink);
    fts.close(&mut fs).unwrap();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"b"], FTS_PHYSICAL, None).unwrap();
    let second = fts.read(&mut fs).unwrap().unwrap();
    let second_ino = second.ino;
    fts.close(&mut fs).unwrap();
    assert_eq!(first_ino, second_ino);
    assert_eq!(first_nlink, 2);
}

#[test]
fn ftw_walks_preorder_and_stops_on_a_nonzero_callback() {
    let mut fs = tree();
    let mut seen: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut callback =
        |path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, flag: i32| {
            seen.push((path.to_vec(), flag));
            0
        };
    let rv = ftw(&mut fs, b"r", &mut callback, 1).unwrap();
    assert_eq!(rv, 0);
    assert_eq!(seen[0], (b"r".to_vec(), FTW_D));
    assert!(
        seen.iter()
            .any(|(p, flag)| p == b"r/sub/x" && *flag == FTW_F)
    );

    let mut stop = |_path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, _flag: i32| 7;
    assert_eq!(ftw(&mut fs, b"r", &mut stop, 1).unwrap(), 7);
}

#[test]
fn ftw_validates_nfds_and_reports_a_cycle() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.mkdir("r/dir", 0o755);
    fs.mkdir("r/dir/b", 0o755);
    fs.symlink("r/dir/b/up", "..");
    let mut callback =
        |_path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, _flag: i32| 0;
    assert_eq!(
        ftw(&mut fs, b"r", &mut callback, 0).unwrap_err().0,
        EINVAL as u16
    );
    assert_eq!(
        ftw(&mut fs, b"r", &mut callback, 1).unwrap_err().0,
        ELOOP as u16
    );
}

#[test]
fn nftw_depth_order_base_and_level() {
    let mut fs = tree();
    let mut seen: Vec<(Vec<u8>, i32, usize, i64)> = Vec::new();
    let mut callback =
        |path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, flag: i32, ftw: &Ftw| {
            seen.push((path.to_vec(), flag, ftw.base, ftw.level));
            0
        };
    let rv = nftw(&mut fs, b"r", &mut callback, 1, FTW_DEPTH).unwrap();
    assert_eq!(rv, 0);
    let order: Vec<&[u8]> = seen.iter().map(|(p, _, _, _)| p.as_slice()).collect();
    assert_eq!(order, vec![b"r/a".as_slice(), b"r/sub/x", b"r/sub", b"r"]);
    let sub = seen.iter().find(|(p, _, _, _)| p == b"r/sub").unwrap();
    assert_eq!(sub.1, FTW_DP);
    assert_eq!(sub.2, 2);
    assert_eq!(sub.3, 1);
}

#[test]
fn nftw_flags_mount_and_sln() {
    let mut fs = tree();
    fs.mkdir("r/mnt", 0o755);
    fs.file("r/mnt/inner", 1);
    fs.mount("r/mnt", 3);
    fs.symlink("r/broken", "missing");
    let mut seen: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut callback =
        |path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, flag: i32, _ftw: &Ftw| {
            seen.push((path.to_vec(), flag));
            0
        };
    let rv = nftw(&mut fs, b"r", &mut callback, 1, FTW_MOUNT | FTW_CHDIR).unwrap();
    assert_eq!(rv, 0);
    assert!(
        seen.iter()
            .any(|(p, flag)| p == b"r/broken" && *flag == FTW_SLN)
    );
    assert!(!seen.iter().any(|(p, _)| p == b"r/mnt/inner"));
}

#[test]
fn nftw_skips_a_cycle_and_validates_nfds() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.mkdir("r/dir", 0o755);
    fs.mkdir("r/dir/b", 0o755);
    fs.symlink("r/dir/b/up", "..");
    let mut callback =
        |_path: &[u8], _stat: &raven_libc_abi::generated::structs::Stat, _flag: i32, _ftw: &Ftw| 0;
    assert_eq!(nftw(&mut fs, b"r", &mut callback, 1, 0).unwrap(), 0);
    assert_eq!(
        nftw(&mut fs, b"r", &mut callback, 0, 0).unwrap_err().0,
        EINVAL as u16
    );
}

#[test]
fn a_symlink_to_a_missing_target_is_slnone() {
    let mut fs = tree();
    fs.symlink("r/gone", "nowhere");
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"r"],
        FTS_LOGICAL | FTS_COMFOLLOW,
        None,
    )
    .unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    let gone = nodes.iter().find(|n| n.name == b"gone").unwrap();
    assert_eq!(gone.info, FTS_SLNONE);
}

#[test]
fn special_nodes_are_default() {
    let mut fs = FakeFs::new();
    fs.node("fifo", NodeKind::Fifo);
    fs.node("sock", NodeKind::Socket);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"fifo", b"sock"], FTS_PHYSICAL, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert_eq!(nodes[0].info, FTS_DEFAULT);
    assert_eq!(nodes[1].info, FTS_DEFAULT);
}

#[test]
fn stat_failure_is_fs_ns() {
    let mut fs = tree();
    fs.fail_after(fs.op_count() + 1, EIO);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_NS);
    assert_eq!(node.errno, EIO);
    fts.close(&mut fs).unwrap();
}

#[test]
fn the_root_argument_is_trimmed_to_its_last_component() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r/sub/x"], FTS_PHYSICAL, None).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.path, b"r/sub/x");
    assert_eq!(node.name, b"x");
    fts.close(&mut fs).unwrap();
}

#[test]
fn the_root_device_is_the_xdev_reference() {
    let mut fs = tree();
    fs.mkdir("r/other", 0o755);
    fs.file("r/other/f", 1);
    fs.mount("r/other", 10);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL | FTS_XDEV, None).unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert!(!nodes.iter().any(|n| n.path == b"r/other/f"));
    let other = nodes.iter().rev().find(|n| n.path == b"r/other").unwrap();
    assert_eq!(other.info, FTS_DP);
}

#[test]
fn a_root_with_comfollowdir_follows_only_directories() {
    let mut fs = tree();
    fs.symlink("lnkdir", "r/sub");
    fs.symlink("lnkfile", "r/a");
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"lnkdir", b"lnkfile"],
        FTS_PHYSICAL | FTS_COMFOLLOWDIR,
        None,
    )
    .unwrap();
    let nodes = drain(&mut fts, &mut fs);
    fts.close(&mut fs).unwrap();
    assert_eq!(nodes[0].info, FTS_D);
    assert!(nodes.iter().any(|n| n.path == b"lnkdir/x"));
    assert_eq!(nodes.last().map(|n| n.info), Some(FTS_SL));
}
