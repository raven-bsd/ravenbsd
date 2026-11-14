// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 10: the `/etc` database parsers (C51).
//!
//! The body drives every file parser, the line reader and the passwd/group packers, and checks
//! the documented line and count limits.

use raven_libc_nss::{Database, Nsswitch};
use raven_libc_pwd::{
    PwError, pack_group, pack_passwd, parse_group_line, parse_master_passwd_line,
    parse_passwd_line, parse_shadow_line,
};
use raven_libc_resolv::{
    IfNames, LineReader, parse_hosts, parse_hosts_line, parse_options_ext, parse_protocols,
    parse_resolv_conf,
};

use crate::{Cursor, alloc_probe};

struct NoIfNames;

impl IfNames for NoIfNames {
    fn name_to_index(&self, _name: &str) -> Option<u32> {
        None
    }
    fn index_to_name(&self, _idx: u32) -> Option<String> {
        None
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn check_pack(payload: &[u8]) -> usize {
    let mut packed = 0usize;
    if let Some(entry) = parse_passwd_line(payload) {
        for size in [0usize, 1, 15, 1024] {
            let mut buffer = vec![0u8; size];
            match pack_passwd(&entry, &mut buffer) {
                Ok(result) => {
                    assert!(
                        result.len <= buffer.len()
                            && result.name < result.len.max(1)
                            && result.passwd < result.len.max(1)
                            && result.gecos < result.len.max(1)
                            && result.dir < result.len.max(1)
                            && result.shell < result.len.max(1),
                        "P-conf-5: packed passwd fields leave the buffer"
                    );
                    packed = packed.saturating_add(1);
                }
                Err(PwError::Range) => {}
                Err(_) => {}
            }
        }
    }
    if let Some(entry) = parse_group_line(payload) {
        for size in [0usize, 1, 15, 1024] {
            let mut buffer = vec![0u8; size];
            match pack_group(&entry, &mut buffer) {
                Ok(result) => {
                    assert!(
                        result.len <= buffer.len() && result.name < result.len.max(1),
                        "P-conf-5: packed group fields leave the buffer"
                    );
                    packed = packed.saturating_add(1);
                }
                Err(PwError::Range) => {}
                Err(_) => {}
            }
        }
    }
    packed
}

/// Run target 10.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let kind = cursor.u8() % 10;
    let payload = cursor.take(4096);
    let ifnames = NoIfNames;

    match kind {
        0 => {
            let conf = parse_resolv_conf(Some(payload));
            assert!(conf.nns <= 3, "P-conf-3: more than three nameservers");
        }
        1 => {
            let _ = parse_options_ext(payload);
        }
        2 => {
            let _ = parse_hosts(payload, &ifnames);
            let _ = parse_hosts_line(payload, &ifnames);
        }
        3 => {
            let _ = parse_protocols(payload);
        }
        4 => {
            let _ = parse_passwd_line(payload);
            let _ = parse_shadow_line(payload);
            let _ = parse_master_passwd_line(payload);
        }
        5 => {
            let _ = parse_group_line(payload);
        }
        6 => {
            let _ = parse_master_passwd_line(payload);
        }
        7 => {
            let nss = Nsswitch::parse(payload);
            let _ = nss.sources(Database::Hosts).len();
        }
        8 => {
            let _ = check_pack(payload);
        }
        _ => {
            let defaults = Nsswitch::defaults();
            for database in [
                Database::Hosts,
                Database::Passwd,
                Database::Group,
                Database::Services,
                Database::Protocols,
                Database::Networks,
            ] {
                assert!(
                    !defaults.sources(database).is_empty(),
                    "P-conf-1: a default database has no source"
                );
            }
        }
    }

    let mut reader = LineReader::new(payload, 4096);
    let mut total = 0usize;
    let mut previous: Option<usize> = None;
    while let Some(line) = reader.next_line() {
        let at = line.as_ptr() as usize;
        if let Some(last) = previous {
            assert!(at > last, "P-conf-4: LineReader returned a line twice");
        }
        previous = Some(at);
        total = total.saturating_add(line.len());
        assert!(
            total <= payload.len(),
            "P-conf-4: the line lengths exceed the input length"
        );
    }

    alloc_probe::note(hash(&[u64::from(kind), payload.len() as u64, total as u64]));
}

#[cfg(test)]
#[path = "resolver_conf_ut.rs"]
mod tests;
