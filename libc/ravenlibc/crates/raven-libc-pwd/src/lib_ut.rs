// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the passwd/group models, parsers, packing and caches.

use super::*;
use alloc::collections::BTreeMap;
use alloc::vec;
use raven_libc_abi::errno::ENOENT;

struct FakeFiles {
    data: BTreeMap<String, Result<Vec<u8>, Errno>>,
}

impl FakeFiles {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(path.to_string(), Ok(bytes.to_vec()));
    }
}

impl FileSource for FakeFiles {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, Errno> {
        self.data.get(path).cloned().unwrap_or(Err(ENOENT))
    }
}

struct FakePasswd(Vec<Passwd>);

impl PasswdSource for FakePasswd {
    fn by_name(&self, name: &str) -> Result<Option<Passwd>, Errno> {
        Ok(self.0.iter().find(|entry| entry.name == name).cloned())
    }

    fn by_uid(&self, uid: u32) -> Result<Option<Passwd>, Errno> {
        Ok(self.0.iter().find(|entry| entry.uid == uid).cloned())
    }

    fn entries(&self) -> Result<Vec<Passwd>, Errno> {
        Ok(self.0.clone())
    }
}

struct FakeGroups(Vec<Group>);

impl GroupSource for FakeGroups {
    fn by_name(&self, name: &str) -> Result<Option<Group>, Errno> {
        Ok(self.0.iter().find(|entry| entry.name == name).cloned())
    }

    fn by_gid(&self, gid: u32) -> Result<Option<Group>, Errno> {
        Ok(self.0.iter().find(|entry| entry.gid == gid).cloned())
    }

    fn entries(&self) -> Result<Vec<Group>, Errno> {
        Ok(self.0.clone())
    }
}

fn passwd() -> Passwd {
    Passwd {
        name: "alice".to_string(),
        passwd: "x".to_string(),
        uid: 1000,
        gid: 1000,
        gecos: "Alice".to_string(),
        dir: "/home/alice".to_string(),
        shell: "/bin/sh".to_string(),
    }
}

fn group() -> Group {
    Group {
        name: "users".to_string(),
        passwd: "x".to_string(),
        gid: 100,
        mem: vec!["alice".to_string(), "bob".to_string()],
    }
}

#[test]
fn a_passwd_line_parses() {
    let entry = parse_passwd_line(b"alice:x:1000:1000:Alice:/home/alice:/bin/sh").expect("passwd");
    assert_eq!(entry, passwd());
}

#[test]
fn a_passwd_line_allows_empty_gecos_and_shell() {
    let entry = parse_passwd_line(b"a:x:1:2::/:").expect("passwd");
    assert_eq!(entry.gecos, "");
    assert_eq!(entry.shell, "");
}

#[test]
fn a_six_field_passwd_line_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:1:2::/"), None);
}

#[test]
fn an_eight_field_passwd_line_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:1:2::/::/bin/sh"), None);
}

#[test]
fn a_passwd_uid_with_letters_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:1x:2::/:"), None);
}

#[test]
fn a_passwd_uid_overflow_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:99999999999:2::/:"), None);
}

#[test]
fn an_empty_passwd_line_is_rejected() {
    assert_eq!(parse_passwd_line(b""), None);
}

#[test]
fn a_group_line_parses() {
    assert_eq!(parse_group_line(b"users:x:100:alice,bob"), Some(group()));
}

#[test]
fn a_group_with_no_members_parses() {
    let entry = parse_group_line(b"users:x:100:").expect("group");
    assert!(entry.mem.is_empty());
}

#[test]
fn a_group_with_one_member_parses() {
    let entry = parse_group_line(b"users:x:100:alice").expect("group");
    assert_eq!(entry.mem, vec!["alice".to_string()]);
}

#[test]
fn a_group_with_empty_member_slots_skips_them() {
    let entry = parse_group_line(b"users:x:100:alice,,bob,").expect("group");
    assert_eq!(entry.mem, vec!["alice".to_string(), "bob".to_string()]);
}

#[test]
fn a_three_field_group_line_is_rejected() {
    assert_eq!(parse_group_line(b"users:x:100"), None);
}

#[test]
fn a_five_field_group_line_is_rejected() {
    assert_eq!(parse_group_line(b"users:x:100:a:b"), None);
}

#[test]
fn a_group_gid_with_letters_is_rejected() {
    assert_eq!(parse_group_line(b"users:x:1x:"), None);
}

#[test]
fn a_shadow_line_parses() {
    let entry = parse_shadow_line(b"alice:$6$x:19000:0:99999:7:14:20000:0").expect("shadow");
    assert_eq!(entry.namp, "alice");
    assert_eq!(entry.lstchg, 19000);
    assert_eq!(entry.expire, 20000);
}

#[test]
fn an_empty_shadow_field_is_unset() {
    let entry = parse_shadow_line(b"alice:x:::::::").expect("shadow");
    assert_eq!(entry.lstchg, -1);
    assert_eq!(entry.min, -1);
    assert_eq!(entry.expire, -1);
    assert_eq!(entry.flag, 0);
}

#[test]
fn an_eight_field_shadow_line_is_rejected() {
    assert_eq!(parse_shadow_line(b"a:x:1:2:3:4:5:6"), None);
}

#[test]
fn a_ten_field_shadow_line_is_rejected() {
    assert_eq!(parse_shadow_line(b"a:x:1:2:3:4:5:6:0:9"), None);
}

#[test]
fn a_negative_shadow_value_parses() {
    let entry = parse_shadow_line(b"a:x:-1:-2:-3:-4:-5:-6:0").expect("shadow");
    assert_eq!(entry.lstchg, -1);
    assert_eq!(entry.min, -2);
}

#[test]
fn a_master_passwd_line_parses() {
    let (entry, shadow) =
        parse_master_passwd_line(b"alice:x:1000:1000::172800:259200:Alice:/home/alice:/bin/sh")
            .expect("master");
    assert_eq!(entry.name, "alice");
    assert_eq!(entry.uid, 1000);
    assert_eq!(shadow.lstchg, 2);
    assert_eq!(shadow.expire, 3);
    assert_eq!(shadow.min, -1);
}

#[test]
fn empty_master_change_and_expire_are_unset() {
    let (_, shadow) = parse_master_passwd_line(b"a:x:1:2::::gecos:/:/bin/sh").expect("master");
    assert_eq!(shadow.lstchg, -1);
    assert_eq!(shadow.expire, -1);
}

#[test]
fn a_nine_field_master_line_is_rejected() {
    assert_eq!(parse_master_passwd_line(b"a:x:1:2:::gecos:/:/bin/sh"), None);
}

#[test]
fn an_eleven_field_master_line_is_rejected() {
    assert_eq!(
        parse_master_passwd_line(b"a:x:1:2:::0:0:gecos:/:/bin/sh:extra"),
        None
    );
}

#[test]
fn pack_passwd_reports_offsets() {
    let entry = passwd();
    let mut buffer = vec![0u8; 128];
    let packed = pack_passwd(&entry, &mut buffer).expect("pack");
    assert_eq!(&buffer[packed.name..packed.name + 5], b"alice");
    assert_eq!(&buffer[packed.shell..packed.shell + 7], b"/bin/sh");
    assert_eq!(buffer[packed.len - 1], 0);
}

#[test]
fn pack_passwd_needs_the_exact_size() {
    let entry = passwd();
    let mut buffer = vec![0u8; 128];
    let packed = pack_passwd(&entry, &mut buffer).expect("pack");
    let mut exact = vec![0u8; packed.len];
    assert!(pack_passwd(&entry, &mut exact).is_ok());
    let mut short = vec![0u8; packed.len - 1];
    assert_eq!(pack_passwd(&entry, &mut short), Err(PwError::Range));
}

#[test]
fn pack_passwd_handles_empty_strings() {
    let entry = Passwd {
        name: String::new(),
        passwd: String::new(),
        uid: 0,
        gid: 0,
        gecos: String::new(),
        dir: String::new(),
        shell: String::new(),
    };
    let mut buffer = [0u8; 16];
    let packed = pack_passwd(&entry, &mut buffer).expect("pack");
    assert_eq!(packed.len, 5);
}

#[test]
fn pack_group_reports_strings_and_member_offsets() {
    let entry = group();
    let mut buffer = vec![0u8; 256];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    assert_eq!(&buffer[packed.name..packed.name + 5], b"users");
    assert_eq!(packed.mem.len(), 2);
    assert_eq!(&buffer[packed.mem[0]..packed.mem[0] + 5], b"alice");
}

#[test]
fn pack_group_needs_the_exact_size() {
    let entry = group();
    let mut buffer = vec![0u8; 256];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    let mut exact = vec![0u8; packed.len];
    assert!(pack_group(&entry, &mut exact).is_ok());
    let mut short = vec![0u8; packed.len - 1];
    assert_eq!(pack_group(&entry, &mut short), Err(PwError::Range));
}

#[test]
fn pack_group_without_members_still_has_a_null_slot() {
    let entry = Group {
        name: "empty".to_string(),
        passwd: "x".to_string(),
        gid: 1,
        mem: Vec::new(),
    };
    let mut buffer = vec![0u8; 128];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    assert!(packed.mem.is_empty());
    assert!(packed.len > packed.passwd);
}

#[test]
fn files_source_reads_passwd_by_name_and_uid() {
    let mut files = FakeFiles::new();
    files.put(
        "/etc/passwd",
        b"alice:x:1000:1000:Alice:/home/alice:/bin/sh\n",
    );
    let source = FilesSource::new(&files);
    assert!(
        PasswdSource::by_name(&source, "alice")
            .expect("by name")
            .is_some()
    );
    assert!(
        PasswdSource::by_uid(&source, 1000)
            .expect("by uid")
            .is_some()
    );
    assert!(
        PasswdSource::by_name(&source, "bob")
            .expect("by name")
            .is_none()
    );
}

#[test]
fn files_source_reads_group_by_name_and_gid() {
    let mut files = FakeFiles::new();
    files.put("/etc/group", b"users:x:100:alice,bob\n");
    let source = FilesSource::new(&files);
    assert!(
        GroupSource::by_name(&source, "users")
            .expect("by name")
            .is_some()
    );
    assert!(GroupSource::by_gid(&source, 100).expect("by gid").is_some());
}

#[test]
fn a_missing_passwd_file_is_an_error() {
    let files = FakeFiles::new();
    let source = FilesSource::new(&files);
    assert_eq!(PasswdSource::entries(&source), Err(ENOENT));
}

#[test]
fn a_missing_group_file_is_an_error() {
    let files = FakeFiles::new();
    let source = FilesSource::new(&files);
    assert_eq!(GroupSource::entries(&source), Err(ENOENT));
}

#[test]
fn malformed_passwd_lines_are_skipped() {
    let mut files = FakeFiles::new();
    files.put(
        "/etc/passwd",
        b"a:x:1:2::/\nb:x:notnum:2::/:\nalice:x:1000:1000::/:/bin/sh\n",
    );
    let source = FilesSource::new(&files);
    assert_eq!(PasswdSource::entries(&source).expect("entries").len(), 1);
}

#[test]
fn malformed_group_lines_are_skipped() {
    let mut files = FakeFiles::new();
    files.put("/etc/group", b"a:x:1\nalice:x:100:alice\n");
    let source = FilesSource::new(&files);
    assert_eq!(GroupSource::entries(&source).expect("entries").len(), 1);
}

#[test]
fn crlf_user_files_parse() {
    let mut files = FakeFiles::new();
    files.put("/etc/passwd", b"alice:x:1000:1000::/:/bin/sh\r\n");
    let source = FilesSource::new(&files);
    assert_eq!(PasswdSource::entries(&source).expect("entries").len(), 1);
}

#[test]
fn an_overlong_user_line_is_skipped_whole() {
    let mut files = FakeFiles::new();
    let mut data = vec![b'x'; MAX_LINE + 1];
    data.push(b'\n');
    data.extend_from_slice(b"alice:x:1000:1000::/:/bin/sh\n");
    files.put("/etc/passwd", &data);
    let source = FilesSource::new(&files);
    assert_eq!(PasswdSource::entries(&source).expect("entries").len(), 1);
}

#[test]
fn getgrouplist_puts_the_base_gid_first() {
    let groups = FakeGroups(vec![
        Group {
            name: "a".to_string(),
            passwd: "x".to_string(),
            gid: 10,
            mem: vec!["alice".to_string()],
        },
        Group {
            name: "b".to_string(),
            passwd: "x".to_string(),
            gid: 20,
            mem: vec!["bob".to_string()],
        },
    ]);
    let mut out = [0u32; 4];
    assert_eq!(getgrouplist("alice", 5, &groups, &mut out), Ok(2));
    assert_eq!(&out[..2], &[5, 10]);
}

#[test]
fn getgrouplist_reports_too_small() {
    let groups = FakeGroups(vec![Group {
        name: "a".to_string(),
        passwd: "x".to_string(),
        gid: 10,
        mem: vec!["alice".to_string()],
    }]);
    let mut out = [0u32; 1];
    assert_eq!(
        getgrouplist("alice", 5, &groups, &mut out),
        Err(GrpListError::TooSmall { needed: 2 })
    );
}

#[test]
fn getgrouplist_deduplicates_the_base_gid() {
    let groups = FakeGroups(vec![Group {
        name: "a".to_string(),
        passwd: "x".to_string(),
        gid: 5,
        mem: vec!["alice".to_string()],
    }]);
    let mut out = [0u32; 4];
    assert_eq!(getgrouplist("alice", 5, &groups, &mut out), Ok(1));
}

#[test]
fn initgroups_groups_starts_with_the_base_gid() {
    let groups = FakeGroups(vec![Group {
        name: "a".to_string(),
        passwd: "x".to_string(),
        gid: 10,
        mem: vec!["alice".to_string()],
    }]);
    assert_eq!(initgroups_groups("alice", 5, &groups), Ok(vec![5, 10]));
}

#[test]
fn a_five_thousand_member_group_is_handled() {
    let mut members = Vec::new();
    for index in 0..5000 {
        members.push(alloc::format!("u{index}"));
    }
    let entry = Group {
        name: "big".to_string(),
        passwd: "x".to_string(),
        gid: 10,
        mem: members,
    };
    let groups = FakeGroups(vec![entry]);
    let result = initgroups_groups("u4999", 5, &groups).expect("groups");
    assert_eq!(result, vec![5, 10]);
}

#[test]
fn pwcache_returns_the_name_and_caches_it() {
    let passwd = FakePasswd(vec![passwd()]);
    let groups = FakeGroups(vec![]);
    let mut cache = PwCache::new(&passwd, &groups);
    assert_eq!(cache.user_from_uid(1000, "?"), Ok("alice".to_string()));
    assert_eq!(cache.user_from_uid(1000, "?"), Ok("alice".to_string()));
}

#[test]
fn pwcache_returns_noname_for_a_missing_uid() {
    let passwd = FakePasswd(vec![]);
    let groups = FakeGroups(vec![]);
    let mut cache = PwCache::new(&passwd, &groups);
    assert_eq!(cache.user_from_uid(42, "nobody"), Ok("nobody".to_string()));
    assert_eq!(cache.user_from_uid(42, "other"), Ok("other".to_string()));
}

#[test]
fn pwcache_looks_up_groups() {
    let passwd = FakePasswd(vec![]);
    let groups = FakeGroups(vec![group()]);
    let mut cache = PwCache::new(&passwd, &groups);
    assert_eq!(cache.group_from_gid(100, "?"), Ok("users".to_string()));
    assert_eq!(
        cache.group_from_gid(101, "nogroup"),
        Ok("nogroup".to_string())
    );
}

#[test]
fn pwcache_maps_names_to_ids() {
    let passwd = FakePasswd(vec![passwd()]);
    let groups = FakeGroups(vec![group()]);
    let cache = PwCache::new(&passwd, &groups);
    assert_eq!(cache.uid_from_user("alice"), Ok(Some(1000)));
    assert_eq!(cache.gid_from_group("users"), Ok(Some(100)));
    assert_eq!(cache.uid_from_user("nobody"), Ok(None));
}

#[test]
fn shells_come_from_etc_shells() {
    let mut files = FakeFiles::new();
    files.put("/etc/shells", b"# comment\n/bin/sh\n/bin/ksh\n");
    let mut cursor = ShellCursor::new();
    assert_eq!(next_shell(&mut cursor, &files), Some("/bin/sh".to_string()));
    assert_eq!(
        next_shell(&mut cursor, &files),
        Some("/bin/ksh".to_string())
    );
    assert_eq!(next_shell(&mut cursor, &files), None);
}

#[test]
fn a_missing_shells_file_falls_back_to_bin_sh() {
    let files = FakeFiles::new();
    let mut cursor = ShellCursor::new();
    assert_eq!(next_shell(&mut cursor, &files), Some("/bin/sh".to_string()));
    assert_eq!(next_shell(&mut cursor, &files), None);
}

#[test]
fn an_empty_shells_file_falls_back_to_bin_sh() {
    let mut files = FakeFiles::new();
    files.put("/etc/shells", b"# only a comment\n\n");
    let mut cursor = ShellCursor::new();
    assert_eq!(next_shell(&mut cursor, &files), Some("/bin/sh".to_string()));
}

#[test]
fn putpwent_formats_a_line() {
    struct Sink(Vec<u8>);
    impl ByteSink for Sink {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }
    let mut sink = Sink(Vec::new());
    putpwent(&passwd(), &mut sink).expect("putpwent");
    assert_eq!(sink.0, b"alice:x:1000:1000:Alice:/home/alice:/bin/sh\n");
}

#[test]
fn putgrent_formats_a_line() {
    struct Sink(Vec<u8>);
    impl ByteSink for Sink {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }
    let mut sink = Sink(Vec::new());
    putgrent(&group(), &mut sink).expect("putgrent");
    assert_eq!(sink.0, b"users:x:100:alice,bob\n");
}

#[test]
fn a_passwd_uid_of_zero_is_accepted() {
    let entry = parse_passwd_line(b"a:x:0:0::/:/bin/sh").expect("passwd");
    assert_eq!(entry.uid, 0);
}

#[test]
fn a_passwd_uid_with_leading_zeros_is_accepted() {
    let entry = parse_passwd_line(b"a:x:0001:2::/:/bin/sh").expect("passwd");
    assert_eq!(entry.uid, 1);
}

#[test]
fn a_negative_passwd_uid_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:-1:2::/:/bin/sh"), None);
}

#[test]
fn a_plus_passwd_uid_is_rejected() {
    assert_eq!(parse_passwd_line(b"a:x:+1:2::/:/bin/sh"), None);
}

#[test]
fn an_empty_passwd_name_is_accepted_by_the_parser() {
    let entry = parse_passwd_line(b":x:1:2::/:/bin/sh").expect("passwd");
    assert_eq!(entry.name, "");
}

#[test]
fn an_empty_passwd_password_is_accepted_by_the_parser() {
    let entry = parse_passwd_line(b"a::1:2::/:/bin/sh").expect("passwd");
    assert_eq!(entry.passwd, "");
}

#[test]
fn a_group_gid_of_zero_is_accepted() {
    let entry = parse_group_line(b"g:x:0:").expect("group");
    assert_eq!(entry.gid, 0);
}

#[test]
fn an_empty_group_name_is_accepted_by_the_parser() {
    let entry = parse_group_line(b":x:1:").expect("group");
    assert_eq!(entry.name, "");
}

#[test]
fn a_group_member_with_a_space_is_kept() {
    let entry = parse_group_line(b"g:x:1:a b").expect("group");
    assert_eq!(entry.mem, vec!["a b".to_string()]);
}

#[test]
fn a_group_trailing_comma_is_ignored() {
    let entry = parse_group_line(b"g:x:1:a,").expect("group");
    assert_eq!(entry.mem, vec!["a".to_string()]);
}

#[test]
fn an_empty_shadow_name_is_accepted_by_the_parser() {
    let entry = parse_shadow_line(b":x:0:0:0:0:0:0:0").expect("shadow");
    assert_eq!(entry.namp, "");
}

#[test]
fn a_shadow_flag_of_zero_is_zero() {
    let entry = parse_shadow_line(b"a:x:0:0:0:0:0:0:0").expect("shadow");
    assert_eq!(entry.flag, 0);
}

#[test]
fn a_shadow_u64_flag_is_kept() {
    let entry = parse_shadow_line(b"a:x:0:0:0:0:0:0:18446744073709551615").expect("shadow");
    assert_eq!(entry.flag, u64::MAX);
}

#[test]
fn a_malformed_shadow_number_is_unset() {
    let entry = parse_shadow_line(b"a:x:x:0:0:0:0:0:0").expect("shadow");
    assert_eq!(entry.lstchg, -1);
}

#[test]
fn master_change_86400_is_one_day() {
    let (_, shadow) = parse_master_passwd_line(b"a:x:1:2::86400:0:g:/:/bin/sh").expect("master");
    assert_eq!(shadow.lstchg, 1);
}

#[test]
fn master_change_86399_is_zero_days() {
    let (_, shadow) = parse_master_passwd_line(b"a:x:1:2::86399:0:g:/:/bin/sh").expect("master");
    assert_eq!(shadow.lstchg, 0);
}

#[test]
fn master_negative_change_is_unset() {
    let (_, shadow) = parse_master_passwd_line(b"a:x:1:2::-1:0:g:/:/bin/sh").expect("master");
    assert_eq!(shadow.lstchg, -1);
}

#[test]
fn master_empty_gecos_is_allowed() {
    let (entry, _) = parse_master_passwd_line(b"a:x:1:2::0:0::/:/bin/sh").expect("master");
    assert_eq!(entry.gecos, "");
}

#[test]
fn master_empty_dir_and_shell_are_allowed() {
    let (entry, _) = parse_master_passwd_line(b"a:x:1:2::0:0:g::").expect("master");
    assert_eq!(entry.dir, "");
    assert_eq!(entry.shell, "");
}

#[test]
fn pack_passwd_is_never_partial() {
    let entry = passwd();
    let mut buffer = vec![0u8; 256];
    let packed = pack_passwd(&entry, &mut buffer).expect("pack");
    for length in 0..packed.len {
        let mut short = vec![0u8; length];
        assert_eq!(pack_passwd(&entry, &mut short), Err(PwError::Range));
    }
}

#[test]
fn pack_passwd_offsets_are_ascending() {
    let entry = passwd();
    let mut buffer = vec![0u8; 256];
    let packed = pack_passwd(&entry, &mut buffer).expect("pack");
    assert!(packed.name < packed.passwd);
    assert!(packed.passwd < packed.gecos);
    assert!(packed.gecos < packed.dir);
    assert!(packed.dir < packed.shell);
}

#[test]
fn pack_group_is_never_partial() {
    let entry = group();
    let mut buffer = vec![0u8; 256];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    for length in 0..packed.len {
        let mut short = vec![0u8; length];
        assert_eq!(pack_group(&entry, &mut short), Err(PwError::Range));
    }
}

#[test]
fn pack_group_ends_with_a_null_pointer_slot() {
    let entry = group();
    let mut buffer = vec![0u8; 256];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    let start = packed.len - core::mem::size_of::<usize>();
    let slot = &buffer[start..packed.len];
    assert_eq!(slot, &0usize.to_ne_bytes());
}

#[test]
fn pack_group_pointer_slots_are_aligned() {
    let entry = group();
    let mut buffer = vec![0u8; 256];
    let packed = pack_group(&entry, &mut buffer).expect("pack");
    assert_eq!(packed.len % core::mem::size_of::<usize>(), 0);
}

#[test]
fn files_source_reads_multiple_passwd_entries() {
    let mut files = FakeFiles::new();
    files.put("/etc/passwd", b"a:x:1:1::/:/bin/sh\nb:x:2:2::/:/bin/sh\n");
    let source = FilesSource::new(&files);
    assert_eq!(PasswdSource::entries(&source).expect("entries").len(), 2);
}

#[test]
fn files_source_reads_multiple_group_entries() {
    let mut files = FakeFiles::new();
    files.put("/etc/group", b"a:x:1:\nb:x:2:\n");
    let source = FilesSource::new(&files);
    assert_eq!(GroupSource::entries(&source).expect("entries").len(), 2);
}

#[test]
fn an_empty_user_file_has_no_entries() {
    let mut files = FakeFiles::new();
    files.put("/etc/passwd", b"");
    let source = FilesSource::new(&files);
    assert!(PasswdSource::entries(&source).expect("entries").is_empty());
}

#[test]
fn getgrouplist_without_groups_returns_the_base() {
    let groups = FakeGroups(vec![]);
    let mut out = [0u32; 1];
    assert_eq!(getgrouplist("alice", 5, &groups, &mut out), Ok(1));
    assert_eq!(out[0], 5);
}

#[test]
fn getgrouplist_accepts_an_exact_buffer() {
    let groups = FakeGroups(vec![Group {
        name: "a".to_string(),
        passwd: "x".to_string(),
        gid: 10,
        mem: vec!["alice".to_string()],
    }]);
    let mut out = [0u32; 2];
    assert_eq!(getgrouplist("alice", 5, &groups, &mut out), Ok(2));
}

#[test]
fn getgrouplist_reports_the_needed_count() {
    let groups = FakeGroups(vec![
        Group {
            name: "a".to_string(),
            passwd: "x".to_string(),
            gid: 10,
            mem: vec!["alice".to_string()],
        },
        Group {
            name: "b".to_string(),
            passwd: "x".to_string(),
            gid: 20,
            mem: vec!["alice".to_string()],
        },
    ]);
    let mut out = [0u32; 1];
    assert_eq!(
        getgrouplist("alice", 5, &groups, &mut out),
        Err(GrpListError::TooSmall { needed: 3 })
    );
}

#[test]
fn pwcache_evicts_the_oldest_positive_entry() {
    let mut entries = Vec::new();
    for uid in 0..10u32 {
        entries.push(Passwd {
            name: alloc::format!("u{uid}"),
            passwd: "x".to_string(),
            uid,
            gid: uid,
            gecos: String::new(),
            dir: String::new(),
            shell: String::new(),
        });
    }
    let passwd = FakePasswd(entries);
    let groups = FakeGroups(vec![]);
    let mut cache = PwCache::new(&passwd, &groups);
    for uid in 0..10u32 {
        assert_eq!(cache.user_from_uid(uid, "?"), Ok(alloc::format!("u{uid}")));
    }
}

#[test]
fn pwcache_negative_group_is_reused() {
    let passwd = FakePasswd(vec![]);
    let groups = FakeGroups(vec![]);
    let mut cache = PwCache::new(&passwd, &groups);
    assert_eq!(cache.group_from_gid(1, "none"), Ok("none".to_string()));
    assert_eq!(cache.group_from_gid(1, "other"), Ok("other".to_string()));
}

#[test]
fn shells_strip_an_inline_comment() {
    let mut files = FakeFiles::new();
    files.put("/etc/shells", b"/bin/sh # comment\n");
    let mut cursor = ShellCursor::new();
    assert_eq!(next_shell(&mut cursor, &files), Some("/bin/sh".to_string()));
}

#[test]
fn an_overlong_shells_line_is_skipped() {
    let mut files = FakeFiles::new();
    let mut data = vec![b'x'; MAX_LINE + 1];
    data.push(b'\n');
    data.extend_from_slice(b"/bin/sh\n");
    files.put("/etc/shells", &data);
    let mut cursor = ShellCursor::new();
    assert_eq!(next_shell(&mut cursor, &files), Some("/bin/sh".to_string()));
}

#[test]
fn putpwent_handles_empty_strings() {
    struct Sink(Vec<u8>);
    impl ByteSink for Sink {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }
    let entry = Passwd {
        name: String::new(),
        passwd: String::new(),
        uid: 0,
        gid: 0,
        gecos: String::new(),
        dir: String::new(),
        shell: String::new(),
    };
    let mut sink = Sink(Vec::new());
    putpwent(&entry, &mut sink).expect("putpwent");
    assert_eq!(sink.0, b"::0:0:::\n");
}

#[test]
fn putgrent_handles_no_members() {
    struct Sink(Vec<u8>);
    impl ByteSink for Sink {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }
    let entry = Group {
        name: "g".to_string(),
        passwd: "x".to_string(),
        gid: 1,
        mem: Vec::new(),
    };
    let mut sink = Sink(Vec::new());
    putgrent(&entry, &mut sink).expect("putgrent");
    assert_eq!(sink.0, b"g:x:1:\n");
}

#[test]
fn pack_passwd_rejects_an_empty_buffer() {
    let mut buffer = [0u8; 0];
    assert_eq!(pack_passwd(&passwd(), &mut buffer), Err(PwError::Range));
}

#[test]
fn pack_group_rejects_an_empty_buffer() {
    let mut buffer = [0u8; 0];
    assert_eq!(pack_group(&group(), &mut buffer), Err(PwError::Range));
}

#[test]
fn a_negative_shadow_flag_is_rejected() {
    assert_eq!(parse_shadow_line(b"a:x:0:0:0:0:0:0:-1"), None);
}
