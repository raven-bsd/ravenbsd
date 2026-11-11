# Counters and tunables

Status: normative inventory for C19. Every name in this file must exist as a
counter or a typed property, with the stated type, unit, range and default.
C19's completeness test reads the names out of this file. An entry that is
listed but cannot be observed to move is a defect; an entry that exists in
the implementation but is not listed here is reported as an extra.

The **choice** of which counters and properties to expose is informed by
decades of illumos and BSD operational practice (the MIB-II set and the
per-stack property tables). The names, definitions and table layout here are
ours; no donor code, comment or table was copied.

## 1. Counter rules

* Every counter is a `u64` field in `Counters`, `#[non_exhaustive]`.
* An increment is one wrapping add in the path that owns the event. It never
  allocates, takes a lock, formats text, or changes a return value.
* A counter that is a subset of another (for example `retrans_bytes` and
  `retrans_segments`) is incremented at the same event; they must stay
  consistent (`retrans_bytes >= retrans_segments * 1` is guaranteed by the
  event, not by a check).
* "Segments" counts TCP segments on the wire, including pure ACKs, SYN, FIN
  and RST, unless the row says "data segments".
* Counters reset to zero with `counters_reset()`; no per-connection counter is
  ever reset by a reconnect, because counters are stack-wide. Per-connection
  values are exposed by `SocketInfo`, not by `Counters`.

## 2. MIB-style connection counters

| Name | Type | Unit | Increments when | Why an operator cares |
|---|---|---|---|---|
| `tcp.active_opens` | u64 | count | A local `tcp_connect` sends its first SYN. | Connection-rate baseline; a drop with stable traffic suggests resource exhaustion. |
| `tcp.passive_opens` | u64 | count | A listener creates a half-open entry from a SYN. | Inbound demand; ratio to active opens detects a server/scan. |
| `tcp.attempt_fails` | u64 | count | A connection fails before ESTAB (SYN retry exhausted, ICMP hard error, RST in SYN-SENT/SYN-RCVD). | Distinguishes "nobody is connecting" from "connections are being refused". |
| `tcp.estab_resets` | u64 | count | A validated RST aborts an ESTAB connection. | Unexpected resets are the first sign of a middlebox or an attack. |
| `tcp.in_segments` | u64 | count | A syntactically valid TCP segment is accepted for a TCB (after checksum and header parse). | Total load and a denominator for the other counters. |
| `tcp.out_segments` | u64 | count | A TCP segment is handed to the IP layer for transmission. | Load; compare with `in_segments` for a one-way flood. |
| `tcp.in_bytes` | u64 | bytes | Payload bytes accepted in sequence (not duplicate, not out of window). | Throughput denominator that excludes retransmissions. |
| `tcp.out_bytes` | u64 | bytes | Payload bytes first sent (not retransmitted). | Goodput offered to the path. |
| `tcp.retrans_segments` | u64 | count | A data segment is retransmitted (RTO, fast retransmit, SACK recovery, TLP, RACK). | The single best loss signal. |
| `tcp.retrans_bytes` | u64 | bytes | Payload bytes retransmitted. | Distinguishes one large retransmission from many small ones. |
| `tcp.fast_retransmits` | u64 | count | The duplicate-ACK threshold triggers a fast retransmit. | Loss without waiting for RTO; a high value suggests a shallow queue. |
| `tcp.sack_retransmits` | u64 | count | A retransmission is selected by the SACK scoreboard (`NextSeg` rule 1). | Shows that SACK recovery, not RTO, is doing the work. |
| `tcp.in_ooo_segments` | u64 | count | An acceptable data segment arrives above `RCV.NXT` and is queued as a hole. | Reordering or loss on the path. |
| `tcp.in_ooo_bytes` | u64 | bytes | Bytes in those segments. | Hole pressure on the reassembly budget. |
| `tcp.in_dup_segments` | u64 | count | A data segment is entirely below `RCV.NXT` (pure duplicate). | Spurious retransmissions by the peer; a sign of ACK loss. |
| `tcp.in_dup_bytes` | u64 | bytes | Duplicate payload bytes. | Same, weighted by size. |
| `tcp.past_window_drops` | u64 | count | A segment has data at or beyond the right window edge and is dropped. | A peer ignoring our window, or a broken middlebox. |
| `tcp.past_window_bytes` | u64 | bytes | Bytes dropped for that reason. | Scale of the violation. |
| `tcp.zwprobe_sent` | u64 | count | We send a zero-window probe. | A stalled receiver; probes should stop when the application reads. |
| `tcp.in_zwnd_probes` | u64 | count | We receive a one-byte probe at `RCV.NXT-1` while our window is zero. | Our own application is not reading fast enough. |
| `tcp.window_updates` | u64 | count | We send an ACK whose main purpose is a window update (the window grew by the SWS threshold or from zero). | Measures receiver drain and SWS behaviour. |
| `tcp.delayed_acks_sent` | u64 | count | A delayed-ACK timer fires and the ACK is sent. | High values mean traffic is being acknowledged by timer, not by data. |
| `tcp.rtt_updates` | u64 | count | An RTT sample is accepted into the estimator. | Estimator health; should roughly track ACKed flights. |
| `tcp.rtt_no_updates` | u64 | count | An ACK is processed but produces no RTT sample (Karn, no timestamp, ambiguous ACK). | Too many non-updates means the estimator is starved. |
| `tcp.listen_drops` | u64 | count | A SYN is dropped because the listener or the global limit is full and cookies are disabled. | Direct evidence of a SYN flood or an undersized backlog. |
| `tcp.half_open_drops` | u64 | count | A half-open entry is refused, evicted or expires. | The half-open queue is the flood-exposed resource. |
| `tcp.accept_queue_drops` | u64 | count | A completing ACK is dropped because the accept queue is full. | The application is not calling `tcp_accept`; no reset was sent. |

## 3. Internal and diagnostic counters

| Name | Type | Unit | Increments when | Why an operator or a developer cares |
|---|---|---|---|---|
| `tcp.reassembly_timeouts` | u64 | count | A hole set is discarded because its timer expired. | Data was lost and the peer never filled the hole; the connection continues. |
| `tcp.reassembly_overflows` | u64 | count | A new hole would exceed `tcp.reassembly_holes_max`; the segment is dropped. | The peer is sending a pathological reordering pattern. |
| `tcp.acks_for_unsent` | u64 | count | An ACK above `SND.NXT` (or outside the RFC 5961 window) is rejected. | Broken peer or injection; should be near zero. |
| `tcp.zero_window_drops` | u64 | count | New data is dropped because our advertised window is zero. | Should be zero if the receive queue is sized correctly. |
| `tcp.rsts_for_unsent` | u64 | count | A RST is generated because a segment arrived for which no connection state exists (CLOSED/LISTEN bad ACK). | Port scans and stale segments; a spike is worth investigating. |
| `tcp.rst_sent` | u64 | count | Any RST is sent. | Total; the sub-counters above explain why. |
| `tcp.rst_rcvd` | u64 | count | A syntactically valid RST is received. | Compare with `estab_resets`; most RSTs are refused or ignored. |
| `tcp.rst_out_of_window` | u64 | count | A RST is outside the receive window and is silently dropped. | Blind reset attack attempts. |
| `tcp.rst_challenge` | u64 | count | A RST is in the window but its sequence number is not `RCV.NXT`; a challenge ACK is sent. | Attack attempts and middlebox-generated RSTs. |
| `tcp.ack_challenge` | u64 | count | An ACK outside the RFC 5961 window produces a challenge ACK. | Injection attempts; should be near zero on a healthy path. |
| `tcp.challenge_acks_sent` | u64 | count | Any challenge ACK is sent. | Sum of the challenge cases; compared with the suppression counter. |
| `tcp.challenge_ack_suppressed` | u64 | count | A challenge ACK would exceed its rate bucket and is suppressed. | High values mean an active attack or a very confused peer. |
| `tcp.paws_drops` | u64 | count | A segment is dropped because its timestamp is older than `TS.Recent` (outside the 24-day exception). | Reordering beyond PAWS, or a broken timestamp generator. |
| `tcp.option_unknown` | u64 | count | An option kind we do not implement is skipped. | New extensions on the path; harmless but informative. |
| `tcp.option_malformed` | u64 | count | An option has length 0/1 or overruns the header; the segment is rejected. | Malformed or hostile peers; also a fuzzing signal. |
| `tcp.data_after_fin` | u64 | count | Data arrives in CLOSING, LAST-ACK or TIME-WAIT after the peer's FIN. | The peer is retransmitting or violating the close handshake. |
| `tcp.finwait2_timeouts` | u64 | count | The FIN-WAIT-2 timer expires and the socket is closed locally. | Half-closed peers that never send FIN. |
| `tcp.user_timeouts` | u64 | count | A caller-set user timeout expires and the connection is aborted. | Application-level policy, visible to the operator. |
| `tcp.timewait_fires` | u64 | count | The TIME-WAIT timer fires (same event as expiry). | Churn through the TIME-WAIT table. |
| `tcp.timewait_expiries` | u64 | count | A TIME-WAIT TCB is deleted on timer expiry and its port is released. | The live-to-expired transition; `timewait_fires` is the event, this is the completed release. |
| `tcp.timewait_recycled` | u64 | count | The TIME-WAIT table is full and the oldest entry is recycled. | The table is undersized for the connection rate. |
| `tcp.timewait_syn_drops` | u64 | count | A SYN arrives for a TIME-WAIT 4-tuple and the RFC 6191 reuse rules reject it. | Connection reuse is being blocked; usually a stale peer. |
| `tcp.timewait_data_drops` | u64 | count | Data arrives in TIME-WAIT and is ACKed but not delivered. | Stale segments from the old incarnation. |
| `tcp.timewait_rst_ignored` | u64 | count | A RST arrives in TIME-WAIT and is ignored (RFC 1337). | Possible assassination attempt. |
| `tcp.connect_fires` | u64 | count | The connect (SYN) timer fires and a SYN is retransmitted. | Early connection trouble. |
| `tcp.rto_fires` | u64 | count | The retransmission timer fires. | Every firing is a loss event; compare with fast retransmits. |
| `tcp.delack_fires` | u64 | count | The delayed-ACK timer fires. | Should match `delayed_acks_sent`; a mismatch is a bug. |
| `tcp.keepalive_fires` | u64 | count | A keepalive probe is sent. | Idle connections and dead peers. |
| `tcp.keepalive_probes_rcvd` | u64 | count | A keepalive probe (zero-length segment at `RCV.NXT-1`) is received and ACKed. | Distinguishes our keepalive from the peer's; useful when both sides probe. |
| `tcp.zwprobe_fires` | u64 | count | A zero-window probe is sent. | Same event as `zwprobe_sent`; kept because C19 requires per-timer firing counts. |
| `tcp.finwait2_fires` | u64 | count | The FIN-WAIT-2 timer fires (before the connection is closed). | Half-closed peers. |
| `tcp.tlp_fires` | u64 | count | A tail-loss probe is sent. | Tail loss without duplicate ACKs. |
| `tcp.rack_losses` | u64 | count | RACK declares a segment lost by time/reordering, not by the duplicate-ACK count. | The modern loss detector is doing the work. |
| `tcp.reordering_window_growth` | u64 | count | The RACK reordering window grows after a DSACK or a measured reorder. | Path reordering is being learned; should stabilize. |
| `tcp.spurious_retransmits` | u64 | count | DSACK or timestamps show that a retransmission was unnecessary. | A high value means the loss detector is too aggressive. |
| `tcp.spurious_undo` | u64 | count | cwnd/ssthresh are restored after a spurious loss event. | Confirms that the undo path actually ran. |
| `tcp.sack_blocks_sent` | u64 | count | A SACK block is emitted in an ACK. | Receiver-side SACK activity. |
| `tcp.sack_blocks_dropped` | u64 | count | A received SACK block is outside the send window or the range cap; it is discarded. | Hostile or broken SACK patterns. |
| `tcp.syn_cookie_sent` | u64 | count | A SYN-ACK is sent with a cookie ISS. | Half-open pressure is being absorbed. |
| `tcp.syn_cookie_valid` | u64 | count | A returning ACK validates a cookie and the connection is reconstructed. | Cookie path works. |
| `tcp.syn_cookie_invalid` | u64 | count | A returning ACK fails cookie validation. | Forgery or stale cookies; should be small. |
| `tcp.port_alloc_fails` | u64 | count | Ephemeral-port allocation fails and `EADDRNOTAVAIL` is returned. | Port exhaustion; a leading indicator of TIME-WAIT pressure. |
| `tcp.port_alloc_probes` | u64 | count | A probe of the port range is made before a port is accepted. | Measures how close the allocator is to a full scan. |
| `tcp.hash_chain_overflows` | u64 | count | A connection-table collision chain exceeds `tcp.hash_chain_max`. | A peer is colliding the table or the hash secret is weak. |
| `tcp.pmtu_updates` | u64 | count | A validated ICMP fragmentation-needed or packet-too-big lowers the cached path MTU. | Path MTU is being discovered. |
| `tcp.pmtu_invalid` | u64 | count | An ICMP MTU message fails quoted-header validation and is ignored. | Forged or stale ICMP; should be near zero. |
| `tcp.plpmtud_probes` | u64 | count | A PLPMTUD probe is sent. | Search progress; bounded by the documented probe budget. |
| `tcp.plpmtud_losses` | u64 | count | A PLPMTUD probe is lost (not counted as congestion loss). | A black hole or a path below the probe size. |
| `tcp.plpmtud_reprobes` | u64 | count | The search is repeated after the configured interval. | Path improvement is being noticed. |
| `tcp.ecn_ce_marks` | u64 | count | A CE-marked packet is received. | Congestion on the path before drops. |
| `tcp.ecn_ce_react` | u64 | count | We react to ECN with a congestion response (once per RTT). | Should be fewer than or equal to `ecn_ce_marks`. |
| `tcp.ecn_cwr_sent` | u64 | count | A CWR is sent after an ECN response. | Completes the ECN handshake. |
| `tcp.timer_early` | u64 | count | A timer deadline is observed in the past by more than one poll interval. | `poll` was not called often enough; a scheduling/CPU problem. |
| `tcp.timer_missed` | u64 | count | A timer deadline is more than `tcp.timer_late_threshold` late. | A strong signal that the stack is starved. |
| `tcp.in_checksum_errors` | u64 | count | A TCP checksum fails (IPv4 or IPv6 pseudo-header). | Corruption or offload misconfiguration. |
| `tcp.in_header_errors` | u64 | count | A TCP header is truncated, has a data offset below 5, or options are malformed. | Hostile input; also a fuzzing signal. |
| `tcp.out_errors` | u64 | count | The device returns `TxError` for a TCP frame. | Driver/queue problem; the segment will be retransmitted. |

## 4. Tunable properties

The property table is enumerable. Each entry carries a name, a type, minimum,
maximum, default, unit and a stability marker: `COMMITTED` means the name and
meaning are promised; `PRIVATE` may change without notice. A `set_property`
outside the range returns `EINVAL` and applies nothing. A `COMMITTED` name is
asserted by a golden-list test in C19.

The defaults below are the **production** defaults. Several are deliberately
different from a general-purpose BSD: the stack is a library with a caller
that owns memory, so buffer sizes are explicit and bounded.

### 4.1 Lifetimes and queues

| Name | Type | Min | Max | Default | Unit | Stability | Meaning |
|---|---|---|---|---|---|---|---|
| `tcp.time_wait_interval` | uint | 1000 | 600000 | 60000 | ms | COMMITTED | How long a TIME-WAIT TCB is kept (2*MSL with MSL=30 s). |
| `tcp.finwait2_timeout` | uint | 1000 | 600000 | 60000 | ms | COMMITTED | Idle timeout for FIN-WAIT-2 (DEVIATION-01); restarted by data, window updates and duplicate FIN. |
| `tcp.user_timeout` | uint | 0 | 86400000 | 0 | ms | COMMITTED | Abort a connection whose user timeout expires; 0 disables. |
| `tcp.keepalive_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Whether keepalive is available; a connection still opts in through the socket layer. |
| `tcp.keepalive_interval` | uint | 1000 | 864000000 | 7200000 | ms | COMMITTED | Idle time before the first keepalive probe. |
| `tcp.keepalive_probes` | uint | 1 | 255 | 9 | count | COMMITTED | Unanswered probes before `ETIMEDOUT`. |
| `tcp.half_open_limit` | uint | 1 | 1048576 | 1024 | entries | COMMITTED | Per-listener half-open queue limit. |
| `tcp.accept_queue_limit` | uint | 1 | 1048576 | 128 | entries | COMMITTED | Per-listener accept queue limit. |
| `tcp.connection_max` | uint | 1 | 16777216 | 65536 | entries | COMMITTED | Global live connection limit across all listeners. |
| `tcp.timewait_max` | uint | 1 | 16777216 | 65536 | entries | COMMITTED | Separate TIME-WAIT table limit. |
| `tcp.reassembly_timeout` | uint | 100 | 600000 | 60000 | ms | COMMITTED | Drop a hole set after this long. |
| `tcp.reassembly_holes_max` | uint | 1 | 1024 | 32 | holes | PRIVATE | Per-connection out-of-order hole budget. |
| `tcp.timer_late_threshold` | uint | 1 | 60000 | 1000 | ms | PRIVATE | Lateness that increments `tcp.timer_missed`. |

### 4.2 Retransmission and congestion

| Name | Type | Min | Max | Default | Unit | Stability | Meaning |
|---|---|---|---|---|---|---|---|
| `tcp.rto_initial` | uint | 1 | 60000 | 1000 | ms | COMMITTED | RTO before the first RTT sample. |
| `tcp.rto_min` | uint | 1 | 60000 | 200 | ms | COMMITTED | Lower clamp on the computed RTO. The default is 200 ms, not RFC 6298's 1 s SHOULD; set it to 1000 to recover the RFC default (DEVIATION-06). |
| `tcp.rto_max` | uint | 1000 | 3600000 | 60000 | ms | COMMITTED | Upper clamp on RTO and on exponential backoff. |
| `tcp.rto_max_retries` | uint | 1 | 255 | 15 | count | COMMITTED | Consecutive RTOs before `ETIMEDOUT`. |
| `tcp.connect_retries` | uint | 1 | 255 | 6 | count | COMMITTED | SYN retransmissions before `ETIMEDOUT`. |
| `tcp.delack_delay` | uint | 1 | 500 | 40 | ms | COMMITTED | Delayed-ACK timer. |
| `tcp.delack_segments` | uint | 1 | 255 | 2 | segments | COMMITTED | Full-sized segments that trigger an immediate ACK. |
| `tcp.zwprobe_max_interval` | uint | 1000 | 3600000 | 60000 | ms | COMMITTED | Cap on the exponential zero-window probe interval. |
| `tcp.dup_ack_threshold` | uint | 1 | 255 | 3 | count | COMMITTED | Duplicate ACKs before fast retransmit. |
| `tcp.initial_window` | uint | 1 | 64 | 10 | segments | COMMITTED | Initial cwnd (RFC 6928; DEVIATION-04). |
| `tcp.cwnd_max` | uint | 2 | 1073741824 | 16777216 | bytes | COMMITTED | Upper bound on cwnd; the receive window can still be lower. |
| `tcp.cwnd_min` | uint | 1 | 64 | 2 | segments | COMMITTED | Floor after recovery; the RTO loss window is one segment until the first new ACK. |
| `tcp.cc_algorithm` | enum | 0 | 1 | 1 | enum | COMMITTED | 0 = NewReno, 1 = CUBIC. Selection is per stack. |
| `tcp.ignore_path_mtu` | bool | 0 | 1 | 0 | bool | COMMITTED | Disable PMTUD/PLPMTUD and use the interface MTU. |
| `tcp.pmtu_cache_max` | uint | 1 | 65536 | 1024 | entries | PRIVATE | Bounded per-destination PMTU cache. |
| `tcp.pmtu_cache_ttl` | uint | 1000 | 86400000 | 600000 | ms | COMMITTED | How long a discovered path MTU is trusted. |
| `tcp.plpmtud_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Enable packetization-layer probing. |
| `tcp.plpmtud_interval` | uint | 1000 | 86400000 | 600000 | ms | COMMITTED | Re-probe interval for a completed search. |
| `tcp.reordering_window_base` | uint | 0 | 10000 | 10 | ms | COMMITTED | RACK base reordering window. |
| `tcp.tlp_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Enable tail-loss probes. |

### 4.3 Options, MSS and buffers

| Name | Type | Min | Max | Default | Unit | Stability | Meaning |
|---|---|---|---|---|---|---|---|
| `tcp.window_scale_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Offer window scaling on SYN. |
| `tcp.timestamps_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Offer timestamps and use PAWS. |
| `tcp.sack_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Offer SACK and use the scoreboard. |
| `tcp.ecn_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Offer ECN and respond to CE. |
| `tcp.syn_cookies_enable` | bool | 0 | 1 | 1 | bool | COMMITTED | Use SYN cookies when the half-open queue is full. |
| `tcp.syn_cookie_lifetime` | uint | 1000 | 600000 | 60000 | ms | COMMITTED | Cookie validity window; the secret rotates on this interval. |
| `tcp.mss_floor` | uint | 64 | 65535 | 536 | bytes | COMMITTED | Never segment below this MSS; a peer asking for less is clamped and counted. |
| `tcp.mss_default` | uint | 64 | 65535 | 1460 | bytes | COMMITTED | MSS used when the interface MTU does not determine one. |
| `tcp.mss_ceiling` | uint | 64 | 65535 | 8960 | bytes | COMMITTED | Maximum MSS we will announce even on a large-MTU link. |
| `tcp.send_buffer_default` | uint | 2048 | 1073741824 | 65536 | bytes | COMMITTED | Per-connection send queue default. |
| `tcp.send_buffer_max` | uint | 2048 | 1073741824 | 4194304 | bytes | COMMITTED | Hard cap on any send queue, regardless of application requests. |
| `tcp.recv_buffer_default` | uint | 2048 | 1073741824 | 65536 | bytes | COMMITTED | Per-connection receive queue default. |
| `tcp.recv_buffer_max` | uint | 2048 | 1073741824 | 4194304 | bytes | COMMITTED | Hard cap on any receive queue. |
| `tcp.nagle` | bool | 0 | 1 | 1 | bool | COMMITTED | Nagle algorithm default; per-socket `set_nodelay` overrides. |
| `tcp.sack_ranges_max` | uint | 1 | 255 | 16 | ranges | PRIVATE | Scoreboard range cap; exceeding it collapses conservatively. |
| `tcp.urgent_mode` | bool | 0 | 1 | 1 | bool | COMMITTED | Deliver urgent-pointer notifications to the application (C09). |

### 4.4 Ports, listen and security

| Name | Type | Min | Max | Default | Unit | Stability | Meaning |
|---|---|---|---|---|---|---|---|
| `tcp.port_range_first` | uint | 1024 | 65535 | 32768 | port | COMMITTED | First ephemeral port. |
| `tcp.port_range_last` | uint | 1024 | 65535 | 60999 | port | COMMITTED | Last ephemeral port; must be >= first. |
| `tcp.isn_strength` | uint | 0 | 2 | 2 | enum | COMMITTED | 0 = deterministic test mode only, 1 = keyed hash, 2 = keyed hash plus clock. Mode 0 is never the default. |
| `tcp.challenge_ack_burst` | uint | 1 | 100000 | 10 | count | COMMITTED | Per-connection challenge-ACK tokens per interval. |
| `tcp.challenge_ack_interval` | uint | 100 | 3600000 | 5000 | ms | COMMITTED | Per-connection token refill interval. |
| `tcp.challenge_ack_global_burst` | uint | 1 | 1000000 | 1000 | count | COMMITTED | Global challenge-ACK tokens per interval. |
| `tcp.challenge_ack_global_interval` | uint | 100 | 3600000 | 1000 | ms | COMMITTED | Global token refill interval. |
| `tcp.hash_chain_max` | uint | 1 | 1024 | 8 | entries | PRIVATE | Maximum collision chain before `tcp.hash_chain_overflows`. |
| `tcp.port_alloc_attempts` | uint | 1 | 65536 | 64 | count | PRIVATE | Random probes before the allocator falls back to a bounded scan. |

## 5. Per-socket information

`SocketInfo` is not a counter and not a tunable; it is a snapshot for
`netstat`-style tools and C20/C24 asserts. It reports:

| Field | Type | Source |
|---|---|---|
| `state` | `TcpState` | TCB state |
| `local` | `(IpAddr, u16)` | 4-tuple |
| `remote` | `Option<(IpAddr, u16)>` | 4-tuple |
| `send_queue` | usize | bytes in the send queue not yet acknowledged |
| `recv_queue` | usize | bytes queued for the application |
| `cwnd` | u32 | current congestion window, bytes |
| `ssthresh` | u32 | current slow-start threshold, bytes |
| `srtt_ms` | u32 | smoothed RTT, milliseconds; 0 before the first sample |
| `rttvar_ms` | u32 | RTT variance, milliseconds; 0 before the first sample |
| `rto_ms` | u32 | current RTO, milliseconds |
| `mss` | u16 | effective MSS after options and PMTU |
| `retransmits` | u32 | retransmissions on this connection |
| `cc_algorithm` | &'static str | `"newreno"` or `"cubic"` |
| `options` | `OptionFlags` | negotiated `wscale`, `timestamps`, `sack`, `ecn`; timestamps also expose `ts_recent` age |

`SocketInfo` is read-only. `sockets()` enumerates live sockets, including
listeners and separate TIME-WAIT entries, so C17's scale tests can assert
table occupancy.

## 6. Mapping to the MIB names

For operators who expect the standard MIB-II names, the mapping is:

| MIB name (RFC 4022) | Our counter |
|---|---|
| `tcpActiveOpens` | `tcp.active_opens` |
| `tcpPassiveOpens` | `tcp.passive_opens` |
| `tcpAttemptFails` | `tcp.attempt_fails` |
| `tcpEstabResets` | `tcp.estab_resets` |
| `tcpInSegs` / `tcpHCInSegs` | `tcp.in_segments` |
| `tcpOutSegs` / `tcpHCOutSegs` | `tcp.out_segments` |
| `tcpRetransSegs` | `tcp.retrans_segments` |
| `tcpInErrs` | `tcp.in_checksum_errors` + `tcp.in_header_errors` |
| `tcpOutRsts` | `tcp.rst_sent` |
| `tcpInOutOfOrder` (private) | `tcp.in_ooo_segments` |
| `tcpInDup` (private) | `tcp.in_dup_segments` |

Our set is a superset; the MIB mapping is provided so an SNMP agent can be
written without guessing semantics.

## 7. Completeness and proof obligations

C19's test reads this file and asserts:

1. every counter name in §2 and §3 exists in `Counters`;
2. every property name in §4 exists in the property table with the stated
   type, unit, range and default;
3. every `COMMITTED` property name matches the golden list;
4. for every counter there is a scenario that increments it and a sibling
   scenario that leaves it at zero;
5. for every property there is a behavioural test that changes the property
   and observes the promised effect;
6. `counters_reset()` zeroes every counter;
7. `SocketInfo` reports the negotiated options and plausible values during a
   1 MiB transfer.

A property whose effect cannot be observed is removed or reclassified before
C19 is accepted. A counter that cannot be made to move is dead and is
removed. The list above is the baseline; C16, C17, C18, C20 and C24 may add
entries only by a reviewed change to this file.
