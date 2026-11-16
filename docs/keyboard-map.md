<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Keyboard keymaps, modifiers and repeat (S65)

OBSERVED on `stuart/S65` (base `origin/master` `ce2f2f91`). Host tests only; no board claim.

## Path

```
boot report -> raven-usb-hid::Keyboard (layout, modifiers, Caps/Num)
            -> KeyEvent { usage, pressed, ascii }
            -> usb_hid::KeyboardService (console queue + Repeat)
            -> ConsoleInput / repeat bytes
```

The USB core/xHCI are untouched. The real enumeration remains S61's follow-up.

## Layouts

`Layout::Us` is the default; `Layout::Uk` is selected with `Keyboard::with_layout`,
`Keyboard::set_layout`, `BootKeyboard::with_layout` or `KeyboardService::with_layout`.

The maps share the letter/digit/control keys. The differences are:

| Usage | Key | US base / shift | UK base / shift |
|---|---|---|---|
| 0x1F | `2` | `2` / `@` | `2` / `"` |
| 0x20 | `3` | `3` / `#` | `3` / `£` (0xA3) |
| 0x28 | `'` | `'` / `"` | `'` / `@` |
| 0x31 | `\ |` | `\` / `|` | `#` / `~` |
| 0x32 | non-US `# ~` | `#` / `~` | `#` / `~` |
| 0x64 | non-US `\ |` | `\` / `|` | `\` / `|` |

US letters are `a..z`; Shift XOR Caps selects upper case. Punctuation uses the shifted table when
Shift is held. UK `£` is emitted as the Latin-1 byte 0xA3 because the console queue is byte
oriented. Keypad `0x54..0x58` (`/ * - + Enter`) and `0x67` (`=`) always map; keypad digits
`0x59..0x62` and `.` (`0x63`) require Num Lock.

## Modifiers

| Modifier | Bits | Behaviour |
|---|---|---|
| Shift | 0x02, 0x20 | Letters: Shift XOR Caps; punctuation: shifted table |
| Control | 0x01, 0x10 | Letters -> `0x01..0x1a`; `[`/`\`/`]` -> `0x1b/0x1c/0x1d`; `^`/`6` -> `0x1e`; `_`/`-` -> `0x1f`; `?` -> `0x7f`; `@`/`2` -> `0x00` |
| Alt (left) | 0x04 | Suppresses ASCII (application shortcut) |
| AltGr (right Alt) | 0x40 | Level-3; US/UK level-3 characters are non-ASCII (€, dead keys) -> no byte; the modifier is tracked |
| GUI | 0x08, 0x80 | Tracked; no ASCII |

`Keyboard::shift/control/alt/altgr/gui()` expose the state; modifier transitions are emitted as
`KeyEvent` presses/releases with no ASCII.

## Locks

* **Caps Lock** (usage `0x39`) toggles on its press and applies to letters (`shift XOR caps`).
* **Num Lock** (usage `0x53`) toggles on its press. On: keypad digits and `.`. Off: those keys are
  navigation keys with no ASCII; `/ * - + Enter =` still map.

Both toggles are applied before the other keys of the same report are mapped.

## Repeat

`usb_hid::Repeat` keeps the last ASCII key pressed:

* default delay 500 ms, period 50 ms (20/s); `RepeatSettings::from_millis` or a custom value;
* a zero period disables repetition;
* a new ASCII key replaces the held one; pressing a modifier/non-ASCII key does not cancel it;
* releasing the held key stops it;
* `KeyboardService::poll_at(now_ns)` feeds presses/releases and `tick(now_ns)` emits one repeat
  byte when due. The compatible `poll()` uses time 0 and never repeats.

## Host evidence

```
$ cargo test -p raven-usb-hid
running 37 tests
test result: ok. 37 passed; 0 failed

$ cargo test -p usb-hid
running 15 tests
test result: ok. 15 passed; 0 failed

$ make check-fast
check-fast: PASSED
```

Coverage: US/UK base/shift maps, ISO keys, UK `£`, Shift/Ctrl/Alt/AltGr state, Caps and Num lock
toggles, keypad digits vs navigation, every control punctuation mapping, repeat delay/period,
release, replacement and zero-period disable. The S62 `usb-keyboard` qtest remains green because
`poll()` keeps its time-0 behaviour and the fake reports use the US layout.

## Limits

* No board claim; the S61 xHCI binding is still UNVERIFIED.
* AltGr level-3 characters are non-ASCII for US/UK and are not represented in the byte console
  queue; dead keys are not implemented.
* Key repeat is driven by the caller's clock; the target timer integration is a follow-up.
