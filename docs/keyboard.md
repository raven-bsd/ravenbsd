<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# PS/2 keyboard (S59)

OBSERVED on `stuart/S59` (base `origin/master` `b93feaf7`), QEMU x86-64, qtest `keyboard`.

## Path

```
i8042 IRQ1 -> keyboard_isr (IDT 0x23) -> i8042_interrupt
           -> drain port 0x60 -> scancode set 1 translation
           -> console::feed_key(byte) -> shared RX ring + DEBUG_READ waiter
```

The keyboard feeds the same `kernel/src/console.rs` ring as COM1. Serial input is unchanged; a
console client reads both sources through `DEBUG_READ` with no distinction. The i8042 module is
included from `console.rs` with `#[path]`, so `kernel/src/arch/x86_64/mod.rs` and `main.rs` need
no wiring; `console::init` calls it after the IDT and IOAPIC exist.

## Controller initialization

* Disable both ports (`0xAD`, `0xA7`), drain stale output.
* Read the configuration byte (`0x20`), set IRQ1 enable (bit 0) and set-2→set-1 translation
  (bit 6), clear the keyboard-clock-disable bit (bit 4), write it back (`0x60`).
* Enable the keyboard port (`0xAE`).
* Route ISA IRQ 1 through the MADT override/IOAPIC to IDT vector `0x23`, then unmask the GSI.
* A missing controller (`status == 0xff`) or an unroutable GSI is logged; the serial console
  keeps working.

Boot serial:

```
keyboard: i8042 irq 1 gsi 1 vector 0x23
```

## Scancode set 1 (US layout)

| Make | Key | Base | Shift |
|---|---|---|---|
| 0x01 | Escape | 0x1b | 0x1b |
| 0x02-0x0b | `1`…`0` | `1`…`0` | `!@#$%^&*()` |
| 0x0c | `-` | `-` | `_` |
| 0x0d | `=` | `=` | `+` |
| 0x0e | Backspace | 0x08 | 0x08 |
| 0x0f | Tab | 0x09 | 0x09 |
| 0x10-0x19 | q w e r t y u i o p | lower | upper |
| 0x1a | `[` | `[` | `{` |
| 0x1b | `]` | `]` | `}` |
| 0x1c | Enter | `\n` | `\n` |
| 0x1e-0x26 | a s d f g h j k l | lower | upper |
| 0x27 | `;` | `;` | `:` |
| 0x28 | `'` | `'` | `"` |
| 0x29 | `` ` `` | `` ` `` | `~` |
| 0x2b | `\` | `\` | `\|` |
| 0x2c-0x32 | z x c v b n m | lower | upper |
| 0x33 | `,` | `,` | `<` |
| 0x34 | `.` | `.` | `>` |
| 0x35 | `/` | `/` | `?` |
| 0x39 | Space | ` ` | ` ` |
| 0x4a | Keypad `-` | `-` | `-` |
| 0x4e | Keypad `+` | `+` | `+` |

State: make/release bit `0x80`; `0x2A`/`0x36` shift, `0x1D` control (letters become
`0x01..0x1a`), `0x3A` caps lock toggles on make. Letters use shift XOR caps; punctuation uses the
shift table when shift is held. The `0xE0` prefix is consumed and the following extended code is
ignored: arrows, function keys and the right-side modifiers are not mapped yet.

## QEMU evidence

The qtest harness has no keyboard hook, so the case starts `send-keys.py`, which connects to a
QMP Unix socket (`-qmp unix:build/fixtures/keyboard/qmp.sock,server=on,wait=off`), enables QMP,
waits for the guest and sends `sendkey h`, `sendkey i`, `sendkey ret`. The fixture prints:

```
keyboard: ready
keyboard: got h
keyboard: got i
keyboard: got enter
keyboard: ok
init exited (0)
```

`build/qtest/keyboard.json`: result `pass`, six matched, no unmatched and no forbidden hits,
11.1 s.

## Limits

* No hardware claim: QEMU's i8042 model only.
* US layout, no key repeat, no `0xE0` extended keys yet.
* The serial console path and `userland/servers/consd/**` are untouched.
