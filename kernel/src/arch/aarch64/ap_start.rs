// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 application-processor bring-up.
//!
//! PSCI starts a secondary CPU at a **physical** entry point with the MMU off. The stub below
//! runs from the image's physical address, builds a translation regime from the kernel roots,
//! enables the MMU while its own code page is identity-mapped by a dedicated TTBR0 root, then
//! branches to the high-half [`ap_entry`].
//!
//! The identity root is built once on the BSP before the first `CPU_ON` and only maps the 2 MiB
//! block containing the stub. `ap_entry` immediately installs the shared empty user root, so the
//! low identity mapping never survives into a user address space.

use core::sync::atomic::{AtomicBool, Ordering};

use raven_rmm::{Mapper, PageFlags, PageSize, PhysAddr, VirtAddr};

use crate::kprintln;
use crate::memory::{frame, paging};
use crate::sched;
use crate::time;

use super::{gic, percpu, psci};

/// Maximum secondary CPUs.
pub const MAX_CPUS: usize = 64;
/// Per-CPU bring-up stack.
const STACK_SIZE: usize = 16 * 1024;
/// Mailbox slots: kernel root, empty user root, identity root, MAIR, TCR, `ap_entry`.
const MAIL_ROOT: usize = 0;
const MAIL_USER_ROOT: usize = 1;
const MAIL_IDENTITY: usize = 2;
const MAIL_MAIR: usize = 3;
const MAIL_TCR: usize = 4;
const MAIL_ENTRY: usize = 5;

/// Values the secondary stub reads before the MMU is on.
#[unsafe(no_mangle)]
static mut AP_MAILBOX: [u64; 6] = [0; 6];

/// Per-CPU bring-up stacks, addressed physically by the stub and virtually after the switch.
#[unsafe(no_mangle)]
#[allow(dead_code)] // Address-only: the assembly names the symbol.
static mut AP_STACKS: [[u8; STACK_SIZE]; MAX_CPUS] = [[0; STACK_SIZE]; MAX_CPUS];

static STARTED: AtomicBool = AtomicBool::new(false);

core::arch::global_asm!(
    r#"
    .section .text

    /* x0 = PSCI context id (the CPU index).
     *
     * The physical code page is identity-mapped by AP_MAILBOX[MAIL_IDENTITY]; the high-half
     * kernel root is TTBR1. After the MMU is on, SP is moved to the high-half stack and the
     * high-half ap_entry is entered. No stack access occurs between the MMU enable and the SP
     * switch.
     */
    .global secondary_start
    .type secondary_start, %function
secondary_start:
    msr daifset, #0xf
    msr spsel, #1
    mov x19, x0

    /* x10 = virtual - physical offset. `adr` gives the physical address of this label with the
     * MMU off; the adjacent literal holds the link-time virtual address. This handles any loader
     * load address instead of assuming the linker's AT() offset. */
    adr x10, secondary_start
    ldr x9, .Lself_virt
    sub x10, x9, x10

    /* SP = physical(AP_STACKS) + (cpu + 1) * STACK_SIZE - 16 */
    ldr x9, .Lap_stacks
    sub x9, x9, x10
    mov x11, #16384
    madd x9, x19, x11, x9
    add sp, x9, x11
    sub sp, sp, #16

    /* Mailbox physical address. */
    ldr x12, .Lap_mailbox
    sub x12, x12, x10

    ldr x1, [x12, #0]   /* kernel TTBR1 root */
    ldr x2, [x12, #8]   /* empty user root (not used until ap_entry) */
    ldr x3, [x12, #16]  /* identity root for this code page */
    ldr x4, [x12, #24]  /* MAIR_EL1 */
    ldr x5, [x12, #32]  /* TCR_EL1 */
    ldr x7, [x12, #40]  /* ap_entry high-half address */

    msr mair_el1, x4
    msr tcr_el1, x5
    msr ttbr0_el1, x3
    msr ttbr1_el1, x1
    dsb sy
    isb

    /* MMU on, caches on. The next fetch is the identity-mapped physical page. */
    mrs x6, sctlr_el1
    orr x6, x6, #1
    orr x6, x6, #(1 << 2)
    orr x6, x6, #(1 << 12)
    msr sctlr_el1, x6
    isb

    /* Now use the high-half stack and vector table. */
    add sp, sp, x10
    ldr x8, .Lvectors
    msr vbar_el1, x8
    isb
    br x7
.Lself_virt:
    .quad secondary_start
.Lap_stacks:
    .quad AP_STACKS
.Lap_mailbox:
    .quad AP_MAILBOX
.Lvectors:
    .quad __exception_vectors
    .size secondary_start, . - secondary_start
"#
);

unsafe extern "C" {
    fn secondary_start();
}

fn write_mailbox(index: usize, value: u64) {
    // SAFETY: only the BSP writes the mailbox before CPU_ON; the APs read it after the release
    // store in `STARTED`/`CPU_ON`.
    unsafe {
        let slot = core::ptr::addr_of_mut!(AP_MAILBOX).cast::<u64>().add(index);
        core::ptr::write_volatile(slot, value);
    }
}

fn read_mailbox(index: usize) -> u64 {
    // SAFETY: `ap_entry` runs after the BSP published every slot; reads are volatile because the
    // write happened on another CPU before PSCI started this one.
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(AP_MAILBOX).cast::<u64>().add(index)) }
}

/// Build the low identity root for the stub's physical code page.
fn identity_root(stub_phys: u64) -> Option<u64> {
    let root = frame::alloc_zeroed_frame()?;
    let mut allocator = paging::KernelFrameAllocator;
    let memory = paging::KernelPhysMemory;
    let mut tlb = paging::KernelTlb;
    let mut mapper: paging::KernelMapper<'_> =
        Mapper::with_encoding(PhysAddr(root.0), &mut allocator, &memory, &mut tlb).ok()?;
    let block = stub_phys & !(0x20_0000 - 1);
    let virt = VirtAddr::new(block)?;
    mapper
        .map(
            virt,
            PhysAddr(block),
            PageFlags::kernel_rx(),
            PageSize::Size2M,
        )
        .ok()?;
    Some(root.0)
}

/// Start every secondary CPU the firmware exposes. Returns the number online, at least one.
pub fn bring_up() -> usize {
    if STARTED.swap(true, Ordering::AcqRel) {
        return crate::sched::smp::online_cpus();
    }
    percpu::install(0);
    percpu::mark_online(0);
    // SAFETY: called once from the boot processor after paging and the frame allocator exist.
    #[cfg(raven_pi4)]
    {
        // The Pi 4 uses a spin-table release, not PSCI; the board run stays UNVERIFIED.
        return 1;
    }
    #[cfg(not(raven_pi4))]
    {
        let Some(root) = paging::kernel_root() else {
            kprintln!("smp: no kernel root; running on the boot processor");
            return 1;
        };
        let Some(user_root) = paging::empty_user_root() else {
            kprintln!("smp: no user root; running on the boot processor");
            return 1;
        };
        let stub = secondary_start as *const () as usize as u64;
        // SAFETY: paging published the copied BootInfo before the scheduler/AP bring-up.
        let boot = unsafe { &*crate::memory::paging::boot_info_ptr() };
        let offset = boot.kernel_virt_base.wrapping_sub(boot.kernel_phys_base);
        let stub_phys = stub.wrapping_sub(offset);
        let Some(identity) = identity_root(stub_phys) else {
            kprintln!("smp: no identity root; running on the boot processor");
            return 1;
        };
        let mair = raven_rmm::arch::aarch64::MAIR_EL1;
        let tcr = raven_rmm::arch::aarch64::tcr_el1(crate::arch::mmu::physical_bits());
        write_mailbox(MAIL_ROOT, root.0);
        write_mailbox(MAIL_USER_ROOT, user_root.0);
        write_mailbox(MAIL_IDENTITY, identity);
        write_mailbox(MAIL_MAIR, mair);
        write_mailbox(MAIL_TCR, tcr);
        write_mailbox(MAIL_ENTRY, ap_entry as *const () as usize as u64);

        let mut online = 1usize;
        let mut cpu = 1usize;
        while cpu < MAX_CPUS {
            let idle = match sched::new_ap_idle() {
                Ok(idle) => idle,
                Err(_) => break,
            };
            if sched::init_cpu(cpu, idle).is_err() {
                break;
            }
            if psci::cpu_on(cpu as u64, stub_phys, cpu as u64).is_err() {
                break;
            }
            let deadline = time::now_ns().saturating_add(500_000_000);
            while !percpu::is_online(cpu) {
                if time::now_ns() > deadline {
                    break;
                }
                core::hint::spin_loop();
            }
            if !percpu::is_online(cpu) {
                kprintln!("smp: cpu {cpu} did not start");
                break;
            }
            online = cpu.saturating_add(1);
            cpu = cpu.saturating_add(1);
        }
        kprintln!("smp: {online} cpus online");
        online
    }
}

/// High-half secondary entry called by the stub.
#[unsafe(no_mangle)]
extern "C" fn ap_entry(cpu: u64) -> ! {
    let cpu = usize::try_from(cpu).unwrap_or(0);
    percpu::install(cpu);
    let _ = gic::init_cpu();
    // SAFETY: the BSP published the empty user root before CPU_ON; switching from the identity
    // root to the shared empty root is the ARMv8-A break-before-make sequence.
    crate::arch::mmu::switch_ttbr0(read_mailbox(MAIL_USER_ROOT), 0);
    let timer_irq = crate::arch::aarch64::irq::timer_irq();
    if timer_irq != 0 {
        let _ = crate::arch::aarch64::timer::init(timer_irq);
        let _ = gic::enable(timer_irq);
    }
    // `ap_main` publishes the idle thread, marks the CPU online and enables interrupts after the
    // per-CPU scheduler slot is the current context.
    sched::ap_main(cpu)
}
