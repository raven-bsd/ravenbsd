<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SNTP client and time-service core

Status: PROPOSED. `crates/raven-sntp` is a host-tested, `no_std`, allocation-free SNTPv4 client
core. It owns the packet codec, NTP era/time conversion, validation, offset/delay computation,
server selection and the timeout/retry policy. It owns no socket, clock syscall or privilege; the
E3 time service injects a UDP transport and a wall-clock implementation.

No live network or QEMU run is part of this ticket. The tests use fixed packets and a mock clock.

## 1. Packet format

SNTPv4 uses the 48-byte NTP packet (RFC 4330/5905). The first word is decoded as:

| bits | field | client request | server reply |
|---|---|---|---|
| 7..6 | leap indicator (`LI`) | 0 | 0..3 |
| 5..3 | version (`VN`) | 4 | 4 |
| 2..0 | mode | 3 (client) | 4 (server) |

The request is 48 bytes with `LI=0`, `VN=4`, `mode=3`, all fields zero except the transmit
timestamp. Timestamps are `seconds.fraction`, both 32-bit big-endian; the fraction is in 2^-32
seconds.

| offset | size | field |
|---:|---:|---|
| 0 | 1 | LI/VN/mode |
| 1 | 1 | stratum |
| 2 | 1 | poll |
| 3 | 1 | precision |
| 4 | 4 | root delay, 16.16 |
| 8 | 4 | root dispersion, 16.16 |
| 12 | 4 | reference ID |
| 16 | 8 | reference timestamp |
| 24 | 8 | originate timestamp (the request transmit echo) |
| 32 | 8 | receive timestamp |
| 40 | 8 | transmit timestamp |

`encode_request` writes `0x23` and the caller's transmit time at bytes 40..48.
`decode_response` rejects a short packet, a version other than 4, and a mode other than 4. The
semantic checks live in `Response::sample`:

* `LI=3` (unsynchronized) and `stratum > 15` are rejected;
* `stratum = 0` (kiss-o'-death, e.g. `RATE`/`DENY`) is rejected;
* the originate timestamp must equal the request transmit timestamp;
* the transmit timestamp must be non-zero;
* the receive/transmit timestamps are converted to `NtpTime` with the request as the era
  reference.

## 2. NTP eras and Unix time

The 32-bit NTP second field wraps at 2036-02-07 06:28:16 UTC. The core keeps the era explicitly:

* `Era::Era0`: 1900-02-07 through 2036-02-07 (wire seconds 0..2^32-1);
* `Era::Era1`: 2036-02-07 through 2172-02-07.

`NtpTime::to_unix_ns()` subtracts the 2,208,988,800-second 1900→1970 offset and adds the era base.
`NtpTime::from_unix_ns()` selects the era from the resulting NTP second count and rejects values
outside the two eras. `NtpTime::from_timestamp()` takes a bare wire timestamp plus the request
transmit time and chooses the era nearest that reference, so an exchange that crosses the 2036
boundary still resolves correctly.

The fixed test vector is 2026-01-01T00:00:00Z = 1,767,225,600 Unix seconds =
3,976,214,400 era-0 NTP seconds; 500 ms encodes as fraction `0x8000_0000`. Era-1 second 10 is
2,085,978,506 Unix seconds.

## 3. Offset and delay

For request transmit `T1`, server receive `T2`, server transmit `T3` and client receive `T4`, all
in Unix nanoseconds:

```text
offset = ((T2 - T1) + (T3 - T4)) / 2
delay  = (T4 - T1) - (T3 - T2)
```

The arithmetic is `i128`, so the 2036 era wrap and the fractional parts do not overflow. A
negative delay is retained in the sample (it is a measurement, not silently retried). The sample
also carries stratum, leap, root delay/dispersion, reference ID and the converted receive/transmit
times.

The fixed exchange `T1=1000 s, T2=1005 s, T3=1006 s, T4=1008 s` yields offset 1.5 s and delay 7 s.

## 4. Server selection and retry

`ServerList` is a fixed four-entry list, so the core stays allocation-free. Each `Server` has an
IPv4 address, UDP port and priority; `select()` returns the lowest priority (first on a tie). A
client pass tries every configured server:

* up to `RetryPolicy::attempts` attempts per server (clamped to 1..=8), each with the policy's
  `timeout_ms`;
* a timeout or an invalid reply advances to the next attempt/server;
* a non-timeout transport failure advances to the next server;
* a valid sample is chosen by lowest stratum, then lowest delay, then server priority;
* after the pass the clock is set once to `clock.now() + best.offset_ns`.

`Transport` is the UDP seam. An E3 implementation opens `net:udp/<ipv4>:123` through netd, writes
the 48-byte request, reads one datagram and returns its length. `resolvd` resolves a configured
server name (`resolv:` A/AAAA) before the transport is built; the core only sees the resulting
IPv4 server entry. A dual-stack deployment adds a second address family to `Server` without
changing the packet codec.

## 5. Wall clock and privilege model

`Clock` is the wall-clock seam:

```rust
pub trait Clock {
    fn now(&self) -> NtpTime;
    fn set(&mut self, time: NtpTime) -> Result<(), Error>;
}
```

The E3 implementation reads `CLOCK_GET` (`0x04`, `REALTIME`) for `now` and calls `CLOCK_SET`
(`0x09`) with a `Timespec` for `set`. `CLOCK_SET` is implemented in
`kernel/src/syscall/basic.rs::clock_set`; only `REALTIME` is settable and a non-positive or
out-of-range value is `EINVAL` (`docs/time.md`). The syscall is not yet a `raven-abi` enum variant.

The current `CLOCK_SET` implementation has no rights check: any process can move `REALTIME`. That is
not the intended demo privilege model. The demo should run the network/SNTP logic unprivileged and
place `Clock::set` in a dedicated **`timed`** service (or an existing supervisor service) that holds
a wall-clock capability. The proposed contract is one of:

1. a `CLOCK_ADMIN`/`ADMIN` right enforced by `clock_set`, with the capability handed to `timed` by
   its service manifest; or
2. a `clock:` scheme whose `set` call is authorized by a transferred capability.

Until the S72 owner chooses, E3 must not run the client with ambient root/ADMIN, must not claim a
privilege check exists, and must keep the `Clock` seam injectable. This is the main remaining owner
input for E3.

Network access is also capability-scoped in the native model: netd owns the NIC and the client opens
only the UDP endpoint on `net:`; the SNTP core never sees a device or an unrestricted socket.

## 6. Scheduling and failure policy

* **Boot:** one synchronization attempt after `netd`/`resolvd` report ready. Boot never blocks on
  SNTP; getty and the demo start from the RTC/`CLOCK_REALTIME` immediately.
* **Periodic:** a background `timed` worker repeats after a poll interval derived from the server
  poll value, clamped to a documented range (for example 64..1024 s). The initial demo may use a
  fixed 1024 s period; the core takes the policy from the service.
* **Failure/backoff:** a failed pass leaves the wall clock unchanged and retries the next server or
  the next period with bounded exponential backoff (cap 30 min). No fake time, no retry that hides
  a stratum-0/KoD or originate mismatch.
* **Offset policy:** a large accepted step moves `REALTIME` once; a later slew policy is a service
  decision, not a packet-core behavior. A rejected/out-of-era value is `EINVAL` at `CLOCK_SET`.
* **No regression:** after a successful set the service verifies a subsequent `CLOCK_GET` is
  monotonic; a backwards step is reported and not retried silently.

E3 wires this schedule into the E-Ink refresh service; E2 only provides the pure core.

## 7. Tests and vectors

`cargo nextest run -p raven-sntp` runs 18 fixed-vector tests:

| vector | expected |
|---|---|
| client request | `0x23` then zeroes, transmit at 40..48 |
| fixed reply | stratum 2, poll 6, precision -20, root delay 0x1000, root dispersion 0x200, refid `GPS ` |
| offset/delay | T1=1000, T2=1005, T3=1006, T4=1008 → offset 1.5 s, delay 7 s |
| 2026 Unix time | 1,767,225,600 s → NTP era 0 / 3,976,214,400 s |
| 500 ms | fraction `0x8000_0000` |
| era 1 | 2,085,978,506 Unix s → era 1 / 10 s |
| era selection | request near `0xFFFF_FF00`, reply `0x0000_0100` → era 1 |
| validation | short/version/mode/LI/stratum 0/stratum 16/originate/zero transmit |
| retry | two timeouts then a reply, 3 calls, clock set to `now + offset` |
| selection | lowest priority, bounded list, stratum > delay > priority tie-break |

The tests use a mock `Clock` and a mock `Transport`; no live network, no QEMU and no real time
source.

## 8. DR-52 donor reconciliation

The packet grammar and arithmetic are from RFC 4330/5905; the donors below are read-only behaviour
references. No C code was copied, translated or closely paraphrased.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Time-discipline API | `usr/src/man/man2/ntp_gettime.2` / `ntp_adjtime.2` and `usr/src/uts/common/syscall/ntptime.c` expose kernel time state; no SNTP client in the checkout (CDDL, read-only) | NetBSD `external/bsd/ntp/dist/sntp/sntp.c` and FreeBSD `contrib/ntp` implement SNTP/NTP (NTP License, permissive; read-only) | `kernel/time/ntp.c` is kernel discipline (GPL, read-only); no client code copied | First-party `raven-sntp`; RFC packet grammar |
| Era/seconds conversion | kernel timex uses 64-bit NTP state; no SNTP era rule to copy | sntp uses the 32-bit wire second field | kernel NTP state | Explicit two-era `NtpTime`; closest-era reference rule |
| Validation | kernel rejects invalid timex state | sntp validates mode/stratum/originate and kiss-o'-death | kernel discipline validates offset state | Reject LI=3, stratum 0/>15, originate mismatch, zero transmit |
| Selection/retry | no client in tree | sntp/ntpdate server lists and retries; no code copied | userspace NTP policy not a donor | Fixed server list, priority, per-server timeout/attempts and best-stratum/delay sample |
| Privilege | `ntp_adjtime` is a privileged kernel time operation (`PRIV_SYS_TIME` in the man page) | BSD `settimeofday`/NTP daemon are root-privileged | `CAP_SYS_TIME` gates wall-clock changes | Client unprivileged; wall-clock set in a dedicated `timed` service with a clock capability; S72 owner input |
| Scheduling | kernel discipline, no client scheduler | ntpd/sntp boot/periodic behavior | kernel/ntpd periodic discipline | Boot one-shot after netd/resolvd, periodic worker, bounded backoff; E3 owns the schedule |

Donor revisions: Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`; NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`; FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`; Linux
`adc218676eef25575469234709c2d87185ca223a`.

## 9. Remaining owner inputs

1. **S72 wall-clock capability.** Choose the `CLOCK_SET` rights check or a `clock:` capability and
   grant it only to `timed`; the current syscall accepts any caller.
2. **E3 service placement.** Confirm `timed` (or the refresh service) owns the periodic worker and
   the `Clock` implementation; the SNTP crate stays policy-free.
3. **Server source.** Confirm the demo's NTP server configuration (static list vs DHCP option 42);
   `resolvd` only resolves names today.
4. **Step/slew policy.** Decide whether the 0.1.0 demo steps the clock or adds slew/limit policy in
   the service.
