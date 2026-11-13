// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Named public-API walk scenarios on the fake tree (the FreeBSD `fts_*_test.c` shape).
#![cfg(feature = "testutil")]

use raven_libc_abi::generated::errno::{EACCES, EINVAL, ENAMETOOLONG};
use raven_libc_abi::generated::structs::Stat;

use raven_libc_fts::fake::{FakeFs, NodeKind};
use raven_libc_fts::{
    AT_FDCWD, Compar, FTS_AGAIN, FTS_COMFOLLOW, FTS_COMFOLLOWDIR, FTS_D, FTS_DC, FTS_DEFAULT,
    FTS_DNR, FTS_DOT, FTS_DP, FTS_ERR, FTS_F, FTS_LOGICAL, FTS_NAMEONLY, FTS_NOINSTR, FTS_NOSTAT,
    FTS_NOSTAT_TYPE, FTS_NS, FTS_NSOK, FTS_PHYSICAL, FTS_SEEDOT, FTS_SKIP, FTS_SL, FTS_SLNONE,
    FTS_WHITEOUT, FTS_XDEV, Fts, FtsEnt, PATH_MAX, ftw, nftw,
};

fn tree() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.file("r/a", 1);
    fs.mkdir("r/sub", 0o755);
    fs.file("r/sub/x", 2);
    fs
}

fn walk(fs: &mut FakeFs, roots: &[&[u8]], options: u32) -> Vec<(i32, Vec<u8>, i64)> {
    let mut fts = Fts::open(fs, AT_FDCWD, roots, options, None).unwrap();
    let mut out = Vec::new();
    while let Some(node) = fts.read(fs).unwrap() {
        out.push((node.info, node.path.clone(), node.level));
    }
    fts.close(fs).unwrap();
    out
}

fn infos(nodes: &[(i32, Vec<u8>, i64)]) -> Vec<i32> {
    nodes.iter().map(|node| node.0).collect()
}

#[test]
fn fts_open_single_file_root() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r/a"], FTS_PHYSICAL);
    assert_eq!(infos(&nodes), vec![FTS_F]);
    assert_eq!(nodes[0].2, 0);
}

#[test]
fn fts_open_dir_root_pre_and_post() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    assert_eq!(
        infos(&nodes),
        vec![FTS_D, FTS_F, FTS_D, FTS_F, FTS_DP, FTS_DP]
    );
}

#[test]
fn fts_open_two_roots_in_argument_order() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r/a", b"r"], FTS_PHYSICAL);
    assert_eq!(nodes[0].1, b"r/a");
    assert_eq!(nodes[1].1, b"r");
}

#[test]
fn fts_root_dot_is_a_directory() {
    let mut fs = tree();
    fs.set_cwd("r");
    let nodes = walk(&mut fs, &[b"."], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_D);
    assert_eq!(nodes[0].1, b".");
}

#[test]
fn fts_root_trailing_slash_is_a_directory() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r/"], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_D);
    assert!(nodes.iter().any(|node| node.1 == b"r/a"));
}

#[test]
fn fts_root_symlink_without_comfollow_is_sl() {
    let mut fs = tree();
    fs.symlink("lnk", "r");
    let nodes = walk(&mut fs, &[b"lnk"], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_SL);
}

#[test]
fn fts_root_symlink_with_comfollow_is_d() {
    let mut fs = tree();
    fs.symlink("lnk", "r");
    let nodes = walk(&mut fs, &[b"lnk"], FTS_PHYSICAL | FTS_COMFOLLOW);
    assert_eq!(nodes[0].0, FTS_D);
    assert!(nodes.iter().any(|node| node.1 == b"lnk/a"));
}

#[test]
fn fts_root_symlink_with_comfollowdir_follows_dirs() {
    let mut fs = tree();
    fs.symlink("lnk", "r");
    fs.symlink("file", "r/a");
    let nodes = walk(&mut fs, &[b"lnk", b"file"], FTS_PHYSICAL | FTS_COMFOLLOWDIR);
    assert_eq!(nodes[0].0, FTS_D);
    assert_eq!(nodes.last().unwrap().0, FTS_SL);
}

#[test]
fn fts_missing_root_is_ns_and_ftw_reports_it() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"missing"], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_NS);
    let mut flag = -1;
    let mut callback = |_path: &[u8], _stat: &Stat, value: i32| {
        flag = value;
        0
    };
    assert_eq!(ftw(&mut fs, b"missing", &mut callback, 1).unwrap(), 0);
    assert_eq!(flag, raven_libc_fts::FTW_NS);
}

#[test]
fn fts_seedot_returns_dot_and_dotdot() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL | FTS_SEEDOT);
    let dots: Vec<&[u8]> = nodes
        .iter()
        .filter(|node| node.0 == FTS_DOT)
        .map(|node| node.1.as_slice())
        .collect();
    assert_eq!(
        dots,
        vec![b"r/.".as_slice(), b"r/..", b"r/sub/.", b"r/sub/.."]
    );
}

#[test]
fn fts_nostat_has_no_stat_buffer() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL | FTS_NOSTAT, None).unwrap();
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert!(node.statp.is_none());
    }
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_nostat_type_implies_nostat() {
    let mut fs = tree();
    fs.symlink("r/l", "a");
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"r"],
        FTS_PHYSICAL | FTS_NOSTAT_TYPE,
        None,
    )
    .unwrap();
    let mut infos = Vec::new();
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert!(node.statp.is_none());
        infos.push(node.info);
    }
    fts.close(&mut fs).unwrap();
    assert!(infos.contains(&FTS_SL));
    assert!(infos.contains(&FTS_F));
}

#[test]
fn fts_whiteout_is_accepted_without_effect() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL | FTS_WHITEOUT);
    assert!(!nodes.iter().any(|node| node.0 == raven_libc_fts::FTS_W));
}

#[test]
fn fts_children_on_a_file_is_empty() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r/a"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    assert!(fts.children(&mut fs, 0).unwrap().is_empty());
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_children_rejects_an_unknown_instruction() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    assert_eq!(
        fts.children(&mut fs, 5).err().map(|error| error.0),
        Some(EINVAL as u16)
    );
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_children_then_read_uses_the_prebuilt_list() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    let children = fts.children(&mut fs, 0).unwrap();
    let first = children[0].path.clone();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.path, first);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_children_nameonly_then_read_rebuilds() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    let names = fts.children(&mut fs, FTS_NAMEONLY as i32).unwrap();
    assert!(names.iter().all(|node| node.statp.is_none()));
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_F);
    assert_eq!(node.path, b"r/a");
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_close_before_read_closes_everything() {
    let mut fs = tree();
    let fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn fts_close_after_a_partial_walk_closes_everything() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn fts_skip_a_file_just_skips_it() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    let a = fts.read(&mut fs).unwrap().unwrap().id;
    fts.set(a, FTS_SKIP).unwrap();
    let next = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(next.path, b"r/sub");
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_again_on_a_directory_repeats_the_preorder() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    let sub = fts.read(&mut fs).unwrap().unwrap().id;
    fts.set(sub, FTS_AGAIN).unwrap();
    let again = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(again.id, sub);
    assert_eq!(again.info, FTS_D);
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_set_noinstr_clears_the_instruction() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let root = fts.read(&mut fs).unwrap().unwrap().id;
    fts.set(root, FTS_NOINSTR).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.path, b"r/a");
    fts.close(&mut fs).unwrap();
}

#[test]
fn fts_xdev_skips_a_mounted_subtree() {
    let mut fs = tree();
    fs.mkdir("r/mnt", 0o755);
    fs.file("r/mnt/f", 1);
    fs.mount("r/mnt", 9);
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL | FTS_XDEV);
    assert!(!nodes.iter().any(|node| node.1 == b"r/mnt/f"));
}

#[test]
fn fts_dnr_is_reported_once_per_directory() {
    let mut fs = tree();
    fs.mkdir("r/locked", 0o000);
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.1 == b"r/locked" && node.0 == FTS_DNR)
            .count(),
        1
    );
}

#[test]
fn fts_dc_is_reported_once_per_cycle() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.mkdir("r/a", 0o755);
    fs.symlink("r/a/up", "..");
    let nodes = walk(&mut fs, &[b"r"], FTS_LOGICAL);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.0 == FTS_DC && node.1 == b"r/a/up")
            .count(),
        1
    );
}

#[test]
fn fts_slnone_is_reported_for_a_dangling_link() {
    let mut fs = tree();
    fs.symlink("r/gone", "nowhere");
    let nodes = walk(&mut fs, &[b"r"], FTS_LOGICAL | FTS_COMFOLLOW);
    assert!(
        nodes
            .iter()
            .any(|node| node.0 == FTS_SLNONE && node.1 == b"r/gone")
    );
}

#[test]
fn fts_special_nodes_are_default() {
    let mut fs = FakeFs::new();
    fs.node("fifo", NodeKind::Fifo);
    fs.node("chr", NodeKind::Char);
    fs.node("blk", NodeKind::Block);
    fs.node("sock", NodeKind::Socket);
    let nodes = walk(&mut fs, &[b"fifo", b"chr", b"blk", b"sock"], FTS_PHYSICAL);
    assert!(nodes.iter().all(|node| node.0 == FTS_DEFAULT));
}

#[test]
fn fts_hard_link_shares_the_inode() {
    let mut fs = FakeFs::new();
    fs.file("a", 3);
    fs.link("b", "a");
    let nodes = walk(&mut fs, &[b"a", b"b"], FTS_PHYSICAL);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"a", b"b"], FTS_PHYSICAL, None).unwrap();
    let first = fts.read(&mut fs).unwrap().unwrap().ino;
    let second = fts.read(&mut fs).unwrap().unwrap().ino;
    fts.close(&mut fs).unwrap();
    assert_eq!(first, second);
    assert_eq!(nodes.len(), 2);
}

#[test]
fn fts_255_byte_name() {
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    let name = "n".repeat(255);
    fs.file(format!("r/{name}"), 1);
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    assert!(
        nodes
            .iter()
            .any(|node| node.1.len() > 255 && node.1.ends_with(name.as_bytes()))
    );
}

#[test]
fn fts_path_cap_marks_the_node_err() {
    let mut fs = FakeFs::new();
    let mut path = String::new();
    for _ in 0..2100 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str("ab");
        fs.mkdir(&path, 0o755);
    }
    let nodes = walk(&mut fs, &[b"ab"], FTS_PHYSICAL);
    assert!(nodes.iter().all(|node| node.1.len() <= PATH_MAX));
    assert!(nodes.iter().any(|node| node.0 == FTS_ERR && node.2 > 0));
}

#[test]
fn fts_root_device_follows_the_root_per_root() {
    let mut fs = tree();
    fs.mkdir("other", 0o755);
    fs.file("other/f", 1);
    fs.mount("other", 5);
    let nodes = walk(&mut fs, &[b"r", b"other"], FTS_PHYSICAL | FTS_XDEV);
    assert!(nodes.iter().any(|node| node.1 == b"other/f"));
}

#[test]
fn fts_comparator_roots_and_children() {
    let mut fs = FakeFs::new();
    fs.mkdir("a", 0o755);
    fs.mkdir("b", 0o755);
    fs.mkdir("a/z", 0o755);
    fs.mkdir("a/y", 0o755);
    let compar: Compar =
        Box::new(
            |left: &FtsEnt, right: &FtsEnt| match left.name.cmp(&right.name) {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            },
        );
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"b", b"a"], FTS_PHYSICAL, Some(compar)).unwrap();
    let roots: Vec<Vec<u8>> = fts
        .children(&mut fs, 0)
        .unwrap()
        .iter()
        .map(|node| node.path.clone())
        .collect();
    assert_eq!(roots, vec![b"a".to_vec(), b"b".to_vec()]);
    fts.close(&mut fs).unwrap();
}

#[test]
fn ftw_preorder_flags() {
    let mut fs = tree();
    let mut seen = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, flag: i32| {
        seen.push((path.to_vec(), flag));
        0
    };
    assert_eq!(ftw(&mut fs, b"r", &mut callback, 1).unwrap(), 0);
    assert_eq!(seen[0].1, raven_libc_fts::FTW_D);
    assert_eq!(seen[1].1, raven_libc_fts::FTW_F);
    assert!(
        seen.iter()
            .any(|(path, flag)| { path == b"r/sub" && *flag == raven_libc_fts::FTW_D })
    );
    assert!(
        seen.iter()
            .any(|(path, flag)| { path == b"r/sub/x" && *flag == raven_libc_fts::FTW_F })
    );
}

#[test]
fn ftw_chdir_flag_is_a_noop() {
    let mut fs = tree();
    let mut paths = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, _flag: i32, _ftw: &raven_libc_fts::Ftw| {
        paths.push(path.to_vec());
        0
    };
    assert_eq!(
        nftw(&mut fs, b"r", &mut callback, 1, raven_libc_fts::FTW_CHDIR).unwrap(),
        0
    );
    assert!(paths.iter().all(|path| path.starts_with(b"r")));
}

#[test]
fn nftw_preorder_by_default() {
    let mut fs = tree();
    let mut order = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, _flag: i32, _ftw: &raven_libc_fts::Ftw| {
        order.push(path.to_vec());
        0
    };
    assert_eq!(nftw(&mut fs, b"r", &mut callback, 1, 0).unwrap(), 0);
    assert_eq!(order[0], b"r");
    assert_eq!(order.last().unwrap(), b"r/sub/x");
}

#[test]
fn nftw_depth_postorder() {
    let mut fs = tree();
    let mut order = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, _flag: i32, _ftw: &raven_libc_fts::Ftw| {
        order.push(path.to_vec());
        0
    };
    assert_eq!(
        nftw(&mut fs, b"r", &mut callback, 1, raven_libc_fts::FTW_DEPTH).unwrap(),
        0
    );
    assert_eq!(order.last().unwrap(), b"r");
}

#[test]
fn nftw_mount_does_not_cross() {
    let mut fs = tree();
    fs.mkdir("r/mnt", 0o755);
    fs.file("r/mnt/f", 1);
    fs.mount("r/mnt", 8);
    let mut crossed = false;
    let mut callback = |path: &[u8], _stat: &Stat, _flag: i32, _ftw: &raven_libc_fts::Ftw| {
        if path == b"r/mnt/f" {
            crossed = true;
        }
        0
    };
    assert_eq!(
        nftw(&mut fs, b"r", &mut callback, 1, raven_libc_fts::FTW_MOUNT).unwrap(),
        0
    );
    assert!(!crossed);
}

#[test]
fn nftw_phys_keeps_a_symlink() {
    let mut fs = tree();
    fs.symlink("r/l", "a");
    let mut flags = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, flag: i32, _ftw: &raven_libc_fts::Ftw| {
        if path == b"r/l" {
            flags.push(flag);
        }
        0
    };
    assert_eq!(
        nftw(&mut fs, b"r", &mut callback, 1, raven_libc_fts::FTW_PHYS).unwrap(),
        0
    );
    assert_eq!(flags, vec![raven_libc_fts::FTW_SL]);
}

#[test]
fn nftw_stop_value_is_returned() {
    let mut fs = tree();
    let mut callback = |_path: &[u8], _stat: &Stat, _flag: i32, _ftw: &raven_libc_fts::Ftw| 42;
    assert_eq!(nftw(&mut fs, b"r", &mut callback, 1, 0).unwrap(), 42);
}

#[test]
fn fts_errors_are_stable_across_an_empty_argv() {
    let mut fs = tree();
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[], FTS_PHYSICAL, None)
            .err()
            .map(|error| error.0),
        Some(EINVAL as u16)
    );
}

#[test]
fn fts_errno_for_an_overlong_root_argument() {
    let mut fs = tree();
    let long = vec![b'a'; PATH_MAX + 1];
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[&long], FTS_PHYSICAL, None)
            .err()
            .map(|error| error.0),
        Some(ENAMETOOLONG as u16)
    );
}

#[test]
fn fts_dnr_errno_is_eacces() {
    let mut fs = tree();
    fs.mkdir("r/locked", 0o000);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    let mut errno = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_DNR {
            errno = node.errno;
        }
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(errno, EACCES);
}

#[test]
fn fts_physical_openat_never_resolves_a_slash_relative_to_a_descriptor() {
    let mut fs = tree();
    let _ = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    for call in &fs.log {
        if call.method == "openat" && call.dirfd != AT_FDCWD {
            assert!(!call.name.contains(&b'/'), "{call:?}");
        }
    }
}

#[test]
fn fts_no_globals_rendering() {
    // A second stream on the same fake is independent of the first.
    let mut fs = tree();
    let first = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    let second = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    assert_eq!(first, second);
}

#[test]
fn fts_whiteout_option_alone_does_not_change_info() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    let white = walk(&mut fs, &[b"r"], FTS_PHYSICAL | FTS_WHITEOUT);
    assert_eq!(infos(&nodes), infos(&white));
}

#[test]
fn fts_default_info_for_unknown_kind() {
    let mut fs = FakeFs::new();
    fs.node("f", NodeKind::Fifo);
    let nodes = walk(&mut fs, &[b"f"], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_DEFAULT);
}

#[test]
fn fts_levels_are_root_zero_and_descendants_count_up() {
    let mut fs = FakeFs::new();
    fs.mkdir("a", 0o755);
    fs.mkdir("a/b", 0o755);
    fs.mkdir("a/b/c", 0o755);
    let nodes = walk(&mut fs, &[b"a"], FTS_PHYSICAL);
    let levels: Vec<i64> = nodes.iter().map(|node| node.2).collect();
    assert_eq!(levels, vec![0, 1, 2, 2, 1, 0]);
}

#[test]
fn fts_nsok_when_nostat_skips_a_leaf() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL | FTS_NOSTAT, None).unwrap();
    let mut saw = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_NSOK {
            saw = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(saw);
}

#[test]
fn fts_no_double_close_of_the_root_descriptor() {
    let mut fs = tree();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"r"], FTS_PHYSICAL, None).unwrap();
    while fts.read(&mut fs).unwrap().is_some() {}
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn fts_errno_zero_for_a_clean_node() {
    let mut fs = tree();
    let nodes = walk(&mut fs, &[b"r"], FTS_PHYSICAL);
    assert!(nodes.iter().all(|node| node.0 != FTS_ERR));
    assert!(nodes.iter().all(|node| node.0 != FTS_NS));
}
