# Header vector arithmetic and parser contract

Status: companion to `headers.toml`. It defines the field names, the order in
which a parser must validate, the exact reason tags, and the checksum
arithmetic. A parser that accepts a `reject` case, or rejects a valid case, is
wrong.

The vectors were constructed field by field from the RFC layouts. Checksums
were computed with the integer one's-complement algorithm below; no packet was
captured, and no stack's output was copied. The work is shown so a reviewer can
recompute a sample by hand.

## 1. Case schema

```toml
[[case]]
name = "..."          # unique, stable; test names use it verbatim
proto = "ipv4"        # ipv4 | ipv6 | tcp | udp | icmp | icmpv6 | arp
bytes = "45 00 ..."   # the complete header, big-endian, space-separated hex
context = { family = "ipv6", src = "...", dst = "..." }  # optional; see below
expect = { ... }      # parsed fields; omitted when reject is present
reject = "tag"        # exact error tag; mutually exclusive with expect
```

`bytes` is the smallest complete unit for the named protocol: a bare TCP or
UDP segment for `proto = "tcp"`/`"udp"`, a complete IP packet for `"ipv4"`/
`"ipv6"`, a complete ARP frame for `"arp"`, and the ICMP message without the
IP header for `"icmp"`/`"icmpv6"`.

For a bare TCP or UDP segment the checksum needs the IP pseudo-header. The
default context is IPv4 `192.0.2.1` -> `198.51.100.2`; a `context` table
overrides the family and addresses. `udp-v6-zero-checksum` uses this to prove
that a zero checksum is rejected only for IPv6.

## 2. Field names

### IPv4 (`proto = "ipv4"`)

| Field | Meaning |
|---|---|
| `version` | 4 |
| `ihl` | header length in 32-bit words, 5..15 |
| `dscp` | 6-bit differentiated services codepoint |
| `ecn` | 2-bit ECN field |
| `total_len` | wire total length, bytes |
| `id` | identification |
| `flags` | `""`, `"DF"`, `"MF"` or `"DF,MF"` in that order |
| `frag_offset` | fragment offset in **bytes** (`wire units * 8`) |
| `ttl` | time to live |
| `protocol` | IP protocol number |
| `checksum` | header checksum as the 16-bit value read from the wire |
| `src`, `dst` | dotted-quad strings |
| `payload_len` | `bytes.len() - ihl*4`, bytes after the header |

Options are not decoded into fields by the generic IPv4 parser; C05 validates
them separately. A parser MUST preserve their bytes in the payload view or
expose `options_len = (ihl-5)*4`; the vector's `payload_len` excludes options.

### IPv6 (`proto = "ipv6"`)

| Field | Meaning |
|---|---|
| `version` | 6 |
| `traffic_class` | 8 bits |
| `flow_label` | 20 bits |
| `payload_len` | wire payload length field |
| `next_header` | first next-header value |
| `hop_limit` | 8 bits |
| `src`, `dst` | full 16-byte addresses, printed as eight 4-hex-digit groups |

An IPv6 parser that follows extension headers reports the first `next_header`
and leaves the extension chain to the next parser. Jumbograms (`payload_len =
0`) are a distinct rejection because this stack does not support them.

### TCP (`proto = "tcp"`)

| Field | Meaning |
|---|---|
| `src_port`, `dst_port` | 16 bits |
| `seq`, `ack` | 32 bits, unsigned |
| `data_offset` | 4 bits, header length in 32-bit words |
| `flags` | comma-separated canonical order: `CWR,ECE,URG,ACK,PSH,RST,SYN,FIN`; empty string if none |
| `window` | raw 16-bit field, unscaled |
| `checksum` | wire checksum |
| `urgent` | raw urgent pointer |
| `options` | array of option tables, in wire order |
| `payload_len` | `bytes.len() - data_offset*4` |

Option tables:

| `name` | Extra fields | Wire |
|---|---|---|
| `eol` | none | kind 0 |
| `nop` | none | kind 1 |
| `mss` | `mss` | kind 2, length 4 |
| `window_scale` | `shift` (0..255 as parsed) | kind 3, length 3 |
| `sack_permitted` | none | kind 4, length 2 |
| `sack` | `blocks` = array of `[left, right]` | kind 5, length 2+8*n |
| `timestamps` | `tsval`, `tsecr` | kind 8, length 10 |
| `unknown` | `kind`, `length` | any other kind with a length |

A `window_scale` value of 15 parses as 15; the **negotiation** layer clamps it
to 14 (RFC 7323 §2.3) and counts the clamp. The vector `tcp-wscale-shift-15`
pins the wire parser, not the negotiation.

### UDP (`proto = "udp"`)

| Field | Meaning |
|---|---|
| `src_port`, `dst_port` | 16 bits |
| `length` | wire UDP length, including the 8-byte header |
| `checksum` | wire checksum; 0 means "not computed" for IPv4 |
| `payload_len` | `bytes.len() - 8` |

### ICMPv4 (`proto = "icmp"`)

| Field | Meaning |
|---|---|
| `type`, `code` | 8 bits each |
| `checksum` | wire checksum over the ICMP message only |
| `rest` | the 32-bit "rest of header" field as an integer |
| `id`, `seq` | echoes only: high and low 16 bits of `rest` |
| `mtu` | type 3 code 4 only: low 16 bits of `rest` |
| `payload_len` | bytes after the 8-byte header |

### ICMPv6 (`proto = "icmpv6"`)

| Field | Meaning |
|---|---|
| `type`, `code` | 8 bits each |
| `checksum` | wire checksum over the IPv6 pseudo-header + message |
| `rest` | 32-bit field |
| `id`, `seq` | echo types 128/129 |
| `mtu` | packet-too-big type 2 |
| `target` | neighbour solicitation/advertisement (types 135/136), first 16 bytes after the header |
| `payload_len` | bytes after the 8-byte header |

### ARP (`proto = "arp"`)

| Field | Meaning |
|---|---|
| `htype`, `ptype` | 16 bits each; this stack accepts only 1 and 0x0800 |
| `hlen`, `plen` | 8 bits each; this stack accepts only 6 and 4 |
| `oper` | 1 request, 2 reply; other values parse but are ignored by the interface |
| `sha`, `tha` | hardware addresses, colon-separated lowercase hex |
| `spa`, `tpa` | IPv4 addresses, dotted quad |

## 3. Validation order and reason tags

The order matters: a malformed packet may violate several rules, and the
vector pins the first one. C21's fuzzer must treat the tag as part of the
contract; changing the order is a spec change.

### IPv4

1. `bytes.len() >= 20`, else `ipv4.truncated`.
2. `version == 4`, else `ipv4.version`.
3. `ihl >= 5` and `ihl*4 <= bytes.len()`, else `ipv4.ihl`.
4. `total_len >= ihl*4` and `total_len <= bytes.len()`, else `ipv4.total_len`.
5. header checksum, else `ipv4.checksum`.

`total_len == 0` is `ipv4.total_len`; padding beyond `total_len` is not an
error (Ethernet padding), but it is not part of the IP payload.

### IPv6

1. `bytes.len() >= 40`, else `ipv6.truncated`.
2. `version == 6`, else `ipv6.version`.
3. `payload_len == 0` -> `ipv6.jumbogram` (unsupported).
4. `40 + payload_len == bytes.len()`, else `ipv6.payload_len`.
5. extension-chain lengths, else `ipv6.ext_len`.

### TCP

1. `bytes.len() >= 20`, else `tcp.truncated`.
2. `data_offset >= 5` and `data_offset*4 <= bytes.len()`, else `tcp.data_offset`.
3. each option: length >= 2 (`tcp.option_length`), fits inside the option area
   (`tcp.option_truncated`), and the kind-specific length is exact:
   MSS 4, WS 3, SACK-permitted 2, SACK 2+8n, timestamps 10
   (`tcp.option_length`).
4. checksum, else `tcp.checksum`.

A `data_offset` of 15 with only 20 bytes is `tcp.data_offset`; a data offset of
5 with a 4-byte option area is impossible because the option area is defined by
the data offset.

### UDP

1. `bytes.len() >= 8`, else `udp.truncated`.
2. `length >= 8` and `length <= bytes.len()`, else `udp.length`.
3. `length == 0` is not a jumbogram here; it is `udp.length`.
4. checksum: with IPv4, zero is accepted; with IPv6, zero is
   `udp.zero_checksum_ipv6`; any other mismatch is `udp.checksum`.

### ICMPv4 / ICMPv6

1. `bytes.len() >= 8`, else `icmp.truncated` / `icmpv6.truncated`.
2. checksum, else `icmp.checksum` / `icmpv6.checksum`.

An unknown type parses successfully; the dispatcher decides whether to act.
ICMPv6 checksum validation needs the source and destination addresses; the
vector context supplies them.

### ARP

1. `bytes.len() >= 28`, else `arp.truncated`.
2. `htype == 1`, else `arp.htype` (no vector; reserved for C21).
3. `ptype == 0x0800`, else `arp.ptype`.
4. `hlen == 6` and `plen == 4`, else `arp.hlen` / `arp.plen` (no `plen` vector;
   C21 adds one if needed).
5. unknown `oper` parses and is ignored by the interface.

## 4. Internet checksum

The checksum is the 16-bit one's-complement of the one's-complement sum of all
16-bit words, with an odd final byte padded on the right with zero. At the
receiver the same sum over the full data, **including** the checksum field,
must be `0xffff`. RFC 1071.

Reference implementation in prose (C02 implements the equivalent, including
the incremental update for a changed field):

```
sum = 0
for each 16-bit word w: sum = sum + w
while sum >> 16: sum = (sum & 0xffff) + (sum >> 16)
checksum = ~sum & 0xffff
```

The pseudo-header is prefixed for TCP, UDP and ICMPv6:

* IPv4: `src(4) | dst(4) | 0x00 | protocol(1) | length(2)`, where length is the
  transport header plus payload, not the IP header.
* IPv6: `src(16) | dst(16) | length(4) | 0x00 0x00 0x00 | next_header(1)`,
  where length is the transport header plus payload.

### 4.1 Worked IPv4 header (`ipv4-tcp-syn`)

Header before the checksum field is set to zero:

```
45 00 | 00 28 | 12 34 | 40 00 | 40 06 | 00 00 | c0 00 | 02 01 | c6 33 | 64 02
```

Sum:

```
0x4500 + 0x0028 = 0x4528
0x4528 + 0x1234 = 0x575c
0x575c + 0x4000 = 0x975c
0x975c + 0x4006 = 0xd762
0xd762 + 0x0000 = 0xd762
0xd762 + 0xc000 = 0x19762 -> fold: 0x9763
0x9763 + 0x0201 = 0x9964
0x9964 + 0xc633 = 0x15f97 -> fold: 0x5f98
0x5f98 + 0x6402 = 0xc39a
checksum = ~0xc39a & 0xffff = 0x3c65
```

The vector's `checksum = 15461` is `0x3c65`.

### 4.2 Worked TCP checksum (`ipv4-tcp-syn`)

Pseudo-header: `c0 00 02 01 | c6 33 64 02 | 00 | 06 | 00 14`.
TCP header with the checksum field zero:

```
9c 40 | 00 50 | 00 00 03 e8 | 00 00 00 00 | 50 02 | ff ff | 00 00 | 00 00
```

The 16-bit sum is `0xdccc`; the complement is `0x2333` (9011), which is the
vector's value. The same computation is used for UDP (`udp-basic` gives
`0x008d`) and ICMPv6 (with its pseudo-header).

### 4.3 Checksum offload

When a device advertises checksum offload, the stack may leave the field at its
pseudo-header value or compute a partial checksum and let the device finish it.
The vectors assume **no offload**: `DeviceCaps.checksum_offload = None`. C02's
parser must still verify a received checksum when the device says it did not
validate it; the vector tests are written for that path.

## 5. Notes that the vectors deliberately pin

* `ipv4-frag-offset` uses wire units 185, exposed as `frag_offset = 1480`
  bytes. A parser that returns 185 has the wrong unit.
* `ipv4-options-nop-eol` has `ihl = 6` and `total_len = 44`; options are four
  bytes (`NOP NOP NOP EOL`). `payload_len = 20` is the TCP segment.
* `ipv4-options-rr` has a 7-byte record-route option plus a one-byte EOL pad;
  `ihl = 7` and `payload_len = 9` (UDP header plus one byte).
* `tcp-syn-all-options` is 28 option bytes: MSS (4), SACK-permitted (2), two
  NOPs, timestamps (10), two NOPs, window scale (3), EOL and four zero pad
  bytes. The parser reports every EOL, including the padding; a sender that
  omits EOL and relies on the data offset is also legal (RFC 9293 §3.1).
* `tcp-sack-two-blocks` has two NOPs so the 18-byte SACK option starts on a
  4-byte boundary; the blocks are `[2000,3000)` and `[5000,6000)`.
* `tcp-wscale-shift-15` parses as 15; negotiation clamps it to 14 (RFC 7323
  §2.3). A shift of 15 is a valid wire value, not a malformed option; this
  spec adds no counter for the clamp.
* `udp-v6-zero-checksum` and `udp-zero-checksum-v4` have identical bytes; only
  `context.family` distinguishes them. That is the point: the zero-checksum
  rule is family-dependent (RFC 8200 §8.1).
* `ipv6-unknown-next` uses next header 59 ("no next header"); it parses and
  the payload is ignored. It is not an error.
* `arp-unknown-op` parses with `oper = 3`; the interface ignores it. A parser
  that rejects unknown operations is too strict for forward compatibility.
* A RST with `window = 0` (`tcp-rst-ack`) is legal; the parser must not reject
  a zero window.

## 6. Counts and coverage

| Protocol | Cases | Valid | Reject | Main malformed reasons covered |
|---|---|---|---|---|
| IPv4 | 16 | 9 | 7 | version, IHL, total length (small/large/zero), checksum, truncation |
| IPv6 | 13 | 8 | 5 | version, truncation, jumbogram, payload length, extension length |
| TCP | 19 | 13 | 6 | data offset (low/high), option length, option truncation, checksum |
| UDP | 8 | 2 | 6 | length (small/large/zero), truncation, checksum, v6 zero checksum |
| ICMPv4 | 8 | 6 | 2 | checksum, truncation |
| ICMPv6 | 9 | 7 | 2 | checksum, truncation |
| ARP | 8 | 5 | 3 | truncation, header length, protocol type |
| **Total** | **81** | **50** | **31** | at least one reject per parser |

C02 consumes these as parse tests; C21 uses the same bytes as fuzz seeds and
must observe the same reason tags. C06/C07 use the ICMP/UDP cases for input
validation. An implementation that adds a new reject reason must add a vector
and a row here in the same commit.
