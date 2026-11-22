// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K298 acceptance tests: named table add, delete and lookup over IPv4 and IPv6.

use raven_pf::{AddrSpec, Conf, ConfError, IpAddr, TableError};

const CONF: &str = "table <web> { 192.0.2.0/24, 2001:db8::1 }\n";

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::parse(&format!("{a}.{b}.{c}.{d}")).unwrap()
}

#[test]
fn table_add_delete_and_lookup_cover_both_families() {
    let mut conf = Conf::parse(CONF).unwrap();
    assert_eq!(conf.table_lookup("web", v4(192, 0, 2, 1)), Ok(true));
    assert_eq!(
        conf.table_lookup("web", IpAddr::parse("2001:db8::1").unwrap()),
        Ok(true)
    );
    assert_eq!(conf.table_lookup("web", v4(198, 51, 100, 1)), Ok(false));

    let host = AddrSpec::parse("198.51.100.7").unwrap();
    conf.table_add("web", host).unwrap();
    assert_eq!(conf.table_lookup("web", v4(198, 51, 100, 7)), Ok(true));

    assert_eq!(conf.table_delete("web", &host), Ok(true));
    assert_eq!(conf.table_lookup("web", v4(198, 51, 100, 7)), Ok(false));
    assert_eq!(conf.table_delete("web", &host), Ok(false));

    let v6 = AddrSpec::parse("2001:db8::2").unwrap();
    conf.table_add("web", v6).unwrap();
    assert_eq!(
        conf.table_lookup("web", IpAddr::parse("2001:db8::2").unwrap()),
        Ok(true)
    );
    assert_eq!(conf.table_delete("web", &v6), Ok(true));
}

#[test]
fn table_names_are_case_insensitive() {
    let mut conf = Conf::parse("table <Web> { 192.0.2.1 }\n").unwrap();
    assert_eq!(conf.table_lookup("web", v4(192, 0, 2, 1)), Ok(true));
    conf.table_add("WEB", AddrSpec::parse("198.51.100.1").unwrap())
        .unwrap();
    assert_eq!(conf.table_lookup("wEb", v4(198, 51, 100, 1)), Ok(true));
}

#[test]
fn unknown_tables_and_nested_entries_are_refused() {
    let mut conf = Conf::parse(CONF).unwrap();
    assert_eq!(
        conf.table_lookup("missing", v4(192, 0, 2, 1)),
        Err(ConfError::UnknownTable)
    );
    assert_eq!(
        conf.table_add("missing", AddrSpec::parse("192.0.2.1").unwrap()),
        Err(ConfError::UnknownTable)
    );
    assert_eq!(
        conf.table_add("web", AddrSpec::Table(0)),
        Err(ConfError::Table(TableError::Nested))
    );
}
