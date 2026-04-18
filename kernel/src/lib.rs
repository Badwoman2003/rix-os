#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

use core::panic::PanicInfo;
pub mod serial;
pub mod qemu;
pub mod console;
pub mod mouse;
pub mod interrupts;
pub mod gdt;
pub mod acpi;
pub mod apic;
pub mod memory;
pub mod bga;
pub mod pcie;
pub mod process;
pub mod scheduler;
pub mod context;
pub mod resources;

pub fn test_panic_handler(info:&PanicInfo)->! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    qemu::shutdown(0x11);
    loop {
        x86_64::instructions::hlt();
    }
}
