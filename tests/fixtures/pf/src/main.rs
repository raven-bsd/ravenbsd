// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K273 pf-core fixture: rules, tables, NAT, state, IPv6 and fail-closed parsing.

#![no_std]
#![no_main]

extern crate alloc;

use raven_pf::{
    AddrSpec, Direction, Engine, Ipv4Addr, Ipv6Addr, Packet, Packet6, Protocol, Verdict, parse,
};
use raven_rt::entry;

entry!(main);

fn run() -> Result<(), ()> {
    let ruleset = parse(
        "table <lan> { 10.0.0.0/8, 192.0.2.1 }\n\
         nat on eth0 from any to any -> 203.0.113.5\n\
         pass in on eth0 proto tcp from <lan> to any keep state\n\
         block in on eth0 proto udp from any to any\n\
         pass out on eth0 proto any from any to any no state\n\
         pass in proto tcp from 2001:db8::/32 to any\n",
    )
    .map_err(|_| ())?;
    if ruleset.tables.len() != 1 || ruleset.rules.len() != 5 {
        return Err(());
    }
    let mut engine = Engine::new(ruleset);

    let allowed = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(192, 0, 2, 1),
        1234,
        Ipv4Addr::new(203, 0, 113, 9),
        443,
        0,
        64,
    );
    if engine.evaluate(&allowed, 0).verdict != Verdict::Pass {
        return Err(());
    }
    let denied = Packet::udp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(198, 51, 100, 1),
        53,
        Ipv4Addr::new(203, 0, 113, 9),
        53,
        64,
    );
    if engine.evaluate(&denied, 0).verdict != Verdict::Block {
        return Err(());
    }

    let reply = Packet::tcp(
        Direction::Out,
        "eth0",
        Ipv4Addr::new(203, 0, 113, 9),
        443,
        Ipv4Addr::new(192, 0, 2, 1),
        1234,
        0,
        64,
    );
    let decision = engine.evaluate(&reply, 1);
    if decision.verdict != Verdict::Pass || decision.state.is_none() {
        return Err(());
    }

    let outbound = Packet::tcp(
        Direction::Out,
        "eth0",
        Ipv4Addr::new(10, 0, 2, 15),
        4000,
        Ipv4Addr::new(198, 51, 100, 1),
        80,
        0,
        64,
    );
    let translation = engine.translate(&outbound).ok_or(())?;
    if translation.destination
        || translation.address != AddrSpec::Host(Ipv4Addr::new(203, 0, 113, 5))
    {
        return Err(());
    }

    let v6 = Packet6 {
        direction: Direction::In,
        interface: "eth0",
        protocol: Protocol::Tcp,
        source: Ipv6Addr::parse("2001:db8::10").map_err(|_| ())?,
        destination: Ipv6Addr::parse("2001:db8:1::20").map_err(|_| ())?,
        source_port: 1234,
        destination_port: 443,
        length: 64,
        tcp_flags: 0,
    };
    if engine.evaluate_v6(&v6).verdict != Verdict::Pass {
        return Err(());
    }

    if parse("pass in proto nope").is_ok() {
        return Err(());
    }

    raven_rt::println!("pf-core: rules=5 table=1");
    raven_rt::println!("pf-core: allow=1 deny=1 state=1 nat=1 v6=1 malformed=1");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(()) => {
            raven_rt::println!("pf-core: fail");
            1
        }
    }
}
