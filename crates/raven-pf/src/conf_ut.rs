// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the runtime table operations on a loaded configuration.

use super::*;
use crate::address::Ipv4Addr;

const CONF: &str = "table <web> { 192.0.2.1 }\n";

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    Ipv4Addr::new(a, b, c, d).into()
}

#[test]
fn table_add_delete_and_lookup_round_trip() {
    let mut conf = Conf::parse(CONF).unwrap();
    assert_eq!(conf.table_lookup("web", v4(192, 0, 2, 1)), Ok(true));
    let v6 = AddrSpec::parse("2001:db8::1").unwrap();
    conf.table_add("web", v6).unwrap();
    assert_eq!(
        conf.table_lookup("web", IpAddr::parse("2001:db8::1").unwrap()),
        Ok(true)
    );
    let one = AddrSpec::parse("192.0.2.1").unwrap();
    assert_eq!(conf.table_delete("web", &one), Ok(true));
    assert_eq!(conf.table_lookup("web", v4(192, 0, 2, 1)), Ok(false));
    assert_eq!(conf.table_delete("web", &one), Ok(false));
}

#[test]
fn an_unknown_table_is_refused() {
    let conf = Conf::parse(CONF).unwrap();
    assert_eq!(
        conf.table_lookup("missing", v4(192, 0, 2, 1)),
        Err(ConfError::UnknownTable)
    );
}

#[test]
fn a_nested_entry_is_refused() {
    let mut conf = Conf::parse(CONF).unwrap();
    assert_eq!(
        conf.table_add("web", AddrSpec::Table(0)),
        Err(ConfError::Table(TableError::Nested))
    );
}
