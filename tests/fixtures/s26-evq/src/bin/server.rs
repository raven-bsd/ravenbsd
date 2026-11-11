// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S26 server: multiplexes two clients, a periodic timer and a transferred user event.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_ipc::protocol::{Opcode, PACKET_SIZE};
use raven_rt::entry;
use raven_rt::println;
use raven_s26_evq::{
    EvqEvent, READABLE, SIGNALED, decode_packet, encode_packet, event_create, event_signal,
    evq_create, evq_ctl, evq_wait, handle_close, recv, scheme_register, send, timer_create,
    timer_set,
};

entry!(main);

/// Handles installed by the controller before this process starts.
const USER_EVENT: Handle = Handle::new(3, 0);
const ACK_EVENT: Handle = Handle::new(4, 0);

const ADD: usize = 1;
const DEL: usize = 3;

const SCHEME_COOKIE: u64 = 0x100;
const TIMER_COOKIE: u64 = 0x200;
const USER_COOKIE: u64 = 0x300;
const DUMMY_COOKIE: u64 = 0xD00D;

/// Opens served before the clients write: the controller and the two clients.
const OPENS: u64 = 3;
/// Writes issued by each client.
const CLIENT_WRITES: u64 = 5_000;
/// Scheme requests the server must answer.
const REQUESTS_TOTAL: u64 = OPENS + 2 * CLIENT_WRITES;
/// Doorbell round trips the signaler performs.
const EVENTS_TOTAL: u64 = 79_997;
/// Total wakeups the loop must count.
const WAKES_TOTAL: u64 = 100_000;

fn main(_args: raven_rt::Args) -> i32 {
    let queue = match evq_create() {
        Ok(queue) => queue,
        Err(_) => return 10,
    };
    if !deregistered_cookie_is_hidden(queue) {
        println!("s26: failed deregistered cookie");
        return 11;
    }
    println!("s26: dereg ok");

    let server = match scheme_register("evq") {
        Ok(server) => server,
        Err(_) => return 12,
    };
    let timer = match timer_create() {
        Ok(timer) => timer,
        Err(_) => return 13,
    };
    if evq_ctl(queue, ADD, server, READABLE, SCHEME_COOKIE).is_err()
        || evq_ctl(queue, ADD, timer, SIGNALED, TIMER_COOKIE).is_err()
        || evq_ctl(queue, ADD, USER_EVENT, SIGNALED, USER_COOKIE).is_err()
    {
        return 14;
    }
    if timer_set(timer, 10_000_000, 10_000_000).is_err() {
        return 15;
    }
    println!("s26: server ready");

    let mut packet_bytes = [0u8; PACKET_SIZE];
    let mut requests = 0u64;
    let mut timers = 0u64;
    let mut events = 0u64;
    let mut output = [EvqEvent::default(); 8];
    loop {
        let count = match evq_wait(queue, &mut output, 5_000_000_000) {
            Ok(count) => count,
            Err(_) => return 16,
        };
        if count == 0 {
            println!("s26: failed wait timeout");
            return 17;
        }
        for event in output.iter().take(count) {
            match event.cookie {
                SCHEME_COOKIE => {
                    let request = match recv(&mut packet_bytes) {
                        Ok(request) => request,
                        Err(_) => return 18,
                    };
                    if !reply(request, &packet_bytes) {
                        return 19;
                    }
                    requests = requests.saturating_add(1);
                }
                TIMER_COOKIE => timers = timers.saturating_add(1),
                USER_COOKIE => {
                    events = events.saturating_add(1);
                    if event_signal(ACK_EVENT).is_err() {
                        return 20;
                    }
                }
                _ => return 21,
            }
        }
        let wakes = requests.saturating_add(timers).saturating_add(events);
        if requests > REQUESTS_TOTAL || events > EVENTS_TOTAL || wakes > WAKES_TOTAL {
            println!("s26: failed counters {} {} {}", requests, timers, events);
            return 22;
        }
        if requests == REQUESTS_TOTAL && events == EVENTS_TOTAL && wakes == WAKES_TOTAL {
            break;
        }
    }
    println!(
        "s26: requests={} timer={} events={} lost=0",
        requests, timers, events
    );
    println!("s26: total=100000");
    let _ = handle_close(queue);
    let _ = handle_close(server);
    let _ = handle_close(timer);
    0
}

fn deregistered_cookie_is_hidden(queue: Handle) -> bool {
    let dummy = match event_create() {
        Ok(dummy) => dummy,
        Err(_) => return false,
    };
    if evq_ctl(queue, ADD, dummy, SIGNALED, DUMMY_COOKIE).is_err() {
        return false;
    }
    if evq_ctl(queue, DEL, dummy, 0, 0).is_err() {
        return false;
    }
    if event_signal(dummy).is_err() {
        return false;
    }
    let mut output = [EvqEvent::default(); 8];
    let count = match evq_wait(queue, &mut output, 1) {
        Ok(count) => count,
        Err(_) => return false,
    };
    !output
        .iter()
        .take(count)
        .any(|event| event.cookie == DUMMY_COOKIE)
}

fn reply(request: Handle, request_bytes: &[u8]) -> bool {
    let mut reply = match decode_packet(request_bytes) {
        Ok(packet) => packet,
        Err(_) => return false,
    };
    reply.status = 0;
    match reply.opcode {
        value if value == Opcode::Open.as_u16() => reply.server_cookie = reply.file_id,
        value if value == Opcode::Write.as_u16() => {}
        value if value == Opcode::Close.as_u16() => {}
        _ => reply.status = i32::from(Errno::EPROTO.0),
    }
    match encode_packet(&reply) {
        Ok(bytes) => send(request, &bytes).is_ok(),
        Err(_) => false,
    }
}
