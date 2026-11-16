// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! i8042 PS/2 keyboard: controller init, IRQ1 and scancode-set-1 translation.
//!
//! The controller is programmed to keep its set-2-to-set-1 translation and to raise ISA IRQ 1.
//! The IRQ handler drains port `0x60`, translates make codes into the console byte stream (US
//! layout, shift and caps lock), feeds the existing console ring and sends the local-APIC EOI.
//! The serial path in [`crate::console`] is unchanged; both inputs share the console queue.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::arch::x86_64::{acpi, apic, idt, ioapic, pic};
use crate::console;
use crate::kprintln;

/// IDT vector for IRQ1 (the serial console uses `0x22`; the driver pool starts at `0x40`).
pub const KEYBOARD_VECTOR: u8 = 0x23;
/// ISA IRQ of the keyboard controller.
const ISA_IRQ: u8 = 1;
/// i8042 data port.
const DATA: u16 = 0x60;
/// i8042 status/command port.
const STATUS: u16 = 0x64;
/// Status bit: output buffer full.
const STATUS_OUTPUT: u8 = 1;
/// Status bit: input buffer full.
const STATUS_INPUT: u8 = 2;
/// Status bit: the controller is absent (reads as 0xff).
const STATUS_ABSENT: u8 = 0xff;
/// Controller configuration: IRQ1 enable.
const CONFIG_IRQ1: u8 = 1 << 0;
/// Controller configuration: set-2 to set-1 translation.
const CONFIG_TRANSLATE: u8 = 1 << 6;
/// Maximum scancodes drained per interrupt; QEMU delivers one per IRQ.
const MAX_DRAIN: usize = 16;
/// Scancode prefix for extended keys.
const EXTENDED: u8 = 0xe0;
/// Left and right shift make codes.
const SHIFT_LEFT: u8 = 0x2a;
/// Right shift make code.
const SHIFT_RIGHT: u8 = 0x36;
/// Caps Lock make code.
const CAPS_LOCK: u8 = 0x3a;
/// Left control make code.
const CONTROL_LEFT: u8 = 0x1d;

/// Shift is held.
static SHIFT: AtomicBool = AtomicBool::new(false);
/// Control is held.
static CONTROL: AtomicBool = AtomicBool::new(false);
/// Caps Lock is active (toggled on its make code).
static CAPS: AtomicBool = AtomicBool::new(false);
/// The previous scancode was the `0xE0` prefix.
static PENDING_EXTENDED: AtomicBool = AtomicBool::new(false);

/// Initialise the controller and route IRQ1.
///
/// Called from [`crate::console::init`] after the IDT and IOAPIC exist and before interrupts are
/// enabled. A missing controller or an unroutable GSI is logged, not fatal: the serial console
/// keeps working.
pub fn init() {
    let status = pic::inb(STATUS);
    if status == STATUS_ABSENT {
        kprintln!("keyboard: i8042 absent");
        return;
    }

    // Disable both ports, drain stale output, read/modify/write the configuration byte.
    command(0xad);
    command(0xa7);
    while pic::inb(STATUS) & STATUS_OUTPUT != 0 {
        let _ = pic::inb(DATA);
    }
    command(0x20);
    let config = read_data();
    let config = (config | CONFIG_IRQ1 | CONFIG_TRANSLATE) & !0x10;
    command(0x60);
    write_data(config);
    command(0xae);

    let gsi = ioapic::gsi_for_isa(ISA_IRQ);
    let (active_low, level) = acpi::info()
        .override_for_isa(ISA_IRQ)
        .unwrap_or((false, false));
    if !idt::set_handler(u16::from(KEYBOARD_VECTOR), isr_address()) {
        kprintln!("keyboard: vector {KEYBOARD_VECTOR:#x} unavailable");
        return;
    }
    match ioapic::route(gsi, KEYBOARD_VECTOR, apic::id(), active_low, level) {
        Ok(()) => match ioapic::unmask(gsi) {
            Ok(()) => {
                kprintln!("keyboard: i8042 irq {ISA_IRQ} gsi {gsi} vector {KEYBOARD_VECTOR:#x}")
            }
            Err(error) => kprintln!("keyboard: irq {ISA_IRQ} unmask failed ({error})"),
        },
        Err(error) => kprintln!("keyboard: irq {ISA_IRQ} route failed ({error})"),
    }
}

/// Send one controller command byte.
fn command(value: u8) {
    wait_input();
    pic::outb(STATUS, value);
}

/// Send one controller data byte.
fn write_data(value: u8) {
    wait_input();
    pic::outb(DATA, value);
}

/// Read one data byte, waiting for the output buffer.
fn read_data() -> u8 {
    for _ in 0..100_000 {
        if pic::inb(STATUS) & STATUS_OUTPUT != 0 {
            return pic::inb(DATA);
        }
        core::hint::spin_loop();
    }
    0
}

/// Wait until the input buffer is empty.
fn wait_input() {
    for _ in 0..100_000 {
        if pic::inb(STATUS) & STATUS_INPUT == 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

/// Rust side of the IRQ1 stub: drain scancodes, feed the console, end the interrupt.
#[unsafe(no_mangle)]
extern "C" fn i8042_interrupt(_rsp: u64) {
    let mut count = 0usize;
    while count < MAX_DRAIN {
        let status = pic::inb(STATUS);
        if status & STATUS_OUTPUT == 0 {
            break;
        }
        let scancode = pic::inb(DATA);
        drain_scancode(scancode);
        count = count.saturating_add(1);
    }
    apic::eoi();
}

/// Translate one scancode and feed the resulting byte, if any.
fn drain_scancode(scancode: u8) {
    if scancode == EXTENDED {
        PENDING_EXTENDED.store(true, Ordering::Release);
        return;
    }
    let extended = PENDING_EXTENDED.swap(false, Ordering::AcqRel);
    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7f;
    match code {
        SHIFT_LEFT | SHIFT_RIGHT => {
            SHIFT.store(!released, Ordering::Release);
            return;
        }
        CONTROL_LEFT => {
            CONTROL.store(!released, Ordering::Release);
            return;
        }
        CAPS_LOCK => {
            if !released {
                let caps = CAPS.load(Ordering::Acquire);
                CAPS.store(!caps, Ordering::Release);
            }
            return;
        }
        _ => {}
    }
    if released || extended {
        return;
    }
    let shift = SHIFT.load(Ordering::Acquire);
    let caps = CAPS.load(Ordering::Acquire);
    let mut byte = map(code, shift, caps);
    if byte == 0 {
        return;
    }
    if CONTROL.load(Ordering::Acquire) && byte.is_ascii_alphabetic() {
        byte = byte.to_ascii_lowercase().wrapping_sub(b'a').wrapping_add(1);
    }
    console::feed_key(byte);
}

/// Scancode-set-1 make-code to ASCII (US layout, unshifted).
const BASE: [u8; 128] = {
    let mut table = [0u8; 128];
    table[0x01] = 0x1b; // Escape
    table[0x02] = b'1';
    table[0x03] = b'2';
    table[0x04] = b'3';
    table[0x05] = b'4';
    table[0x06] = b'5';
    table[0x07] = b'6';
    table[0x08] = b'7';
    table[0x09] = b'8';
    table[0x0a] = b'9';
    table[0x0b] = b'0';
    table[0x0c] = b'-';
    table[0x0d] = b'=';
    table[0x0e] = 0x08; // Backspace
    table[0x0f] = b'\t';
    table[0x10] = b'q';
    table[0x11] = b'w';
    table[0x12] = b'e';
    table[0x13] = b'r';
    table[0x14] = b't';
    table[0x15] = b'y';
    table[0x16] = b'u';
    table[0x17] = b'i';
    table[0x18] = b'o';
    table[0x19] = b'p';
    table[0x1a] = b'[';
    table[0x1b] = b']';
    table[0x1c] = b'\n';
    table[0x1e] = b'a';
    table[0x1f] = b's';
    table[0x20] = b'd';
    table[0x21] = b'f';
    table[0x22] = b'g';
    table[0x23] = b'h';
    table[0x24] = b'j';
    table[0x25] = b'k';
    table[0x26] = b'l';
    table[0x27] = b';';
    table[0x28] = b'\'';
    table[0x29] = b'`';
    table[0x2b] = b'\\';
    table[0x2c] = b'z';
    table[0x2d] = b'x';
    table[0x2e] = b'c';
    table[0x2f] = b'v';
    table[0x30] = b'b';
    table[0x31] = b'n';
    table[0x32] = b'm';
    table[0x33] = b',';
    table[0x34] = b'.';
    table[0x35] = b'/';
    table[0x39] = b' ';
    table[0x4a] = b'-';
    table[0x4e] = b'+';
    table
};

/// Scancode-set-1 make-code to ASCII with shift held.
const SHIFTED: [u8; 128] = {
    let mut table = [0u8; 128];
    table[0x02] = b'!';
    table[0x03] = b'@';
    table[0x04] = b'#';
    table[0x05] = b'$';
    table[0x06] = b'%';
    table[0x07] = b'^';
    table[0x08] = b'&';
    table[0x09] = b'*';
    table[0x0a] = b'(';
    table[0x0b] = b')';
    table[0x0c] = b'_';
    table[0x0d] = b'+';
    table[0x1a] = b'{';
    table[0x1b] = b'}';
    table[0x27] = b':';
    table[0x28] = b'"';
    table[0x29] = b'~';
    table[0x2b] = b'|';
    table[0x33] = b'<';
    table[0x34] = b'>';
    table[0x35] = b'?';
    table
};

/// One make code, with shift/caps applied to letters.
fn map(code: u8, shift: bool, caps: bool) -> u8 {
    let index = usize::from(code);
    let Some(base) = BASE.get(index).copied() else {
        return 0;
    };
    if base.is_ascii_lowercase() {
        let upper = shift ^ caps;
        return if upper {
            base.to_ascii_uppercase()
        } else {
            base
        };
    }
    if shift {
        return SHIFTED.get(index).copied().unwrap_or(base);
    }
    base
}

core::arch::global_asm!(
    r#"
    .section .text
    .global keyboard_isr
    .type keyboard_isr, @function
keyboard_isr:
    test byte ptr [rsp+8], 3
    jz 1f
    swapgs
1:
    cld
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push rbp
    mov rbp, rsp
    mov rdi, rbp
    and rsp, -16
    call i8042_interrupt
    mov rsp, rbp
    pop rbp
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    test byte ptr [rsp+8], 3
    jz 2f
    swapgs
2:
    iretq
    .size keyboard_isr, . - keyboard_isr
"#
);

unsafe extern "C" {
    fn keyboard_isr();
}

fn isr_address() -> u64 {
    keyboard_isr as *const () as usize as u64
}
