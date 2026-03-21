#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;
pub mod serial;
pub mod qemu;

pub fn test_panic_handler(info:&PanicInfo)->! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    qemu::shutdown(0x11);
    loop {
        x86_64::instructions::hlt();
    }
}
