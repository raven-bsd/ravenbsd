// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Credential reporter: prints the effective ids and fails when they differ from the expected pair.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;

entry!(main);

fn main(args: raven_rt::Args) -> i32 {
    let label = args.get(1).unwrap_or("child");
    let expected_euid = parse_id(args.get(2));
    let expected_egid = parse_id(args.get(3));
    let ids = match raven_rt::proc::credentials() {
        Ok(ids) => ids,
        Err(_) => {
            println!("{label}-child: credentials");
            return 1;
        }
    };
    println!("{label}-child: euid={} egid={}", ids.euid, ids.egid);
    if ids.euid == expected_euid && ids.egid == expected_egid {
        0
    } else {
        1
    }
}

fn parse_id(text: Option<&str>) -> u32 {
    text.and_then(|value| value.parse().ok())
        .unwrap_or(u32::MAX)
}
