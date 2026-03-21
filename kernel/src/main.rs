#![no_std]
#![no_main]
#![panic_handler]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

use core::panic::PanicInfo;

use bga::{VBE_DISPI_BPP_32, bga_set_bank, bga_set_video_mode};
use bootloader_api::entry_point;
use log::*;

///use crate::{println, serial_println};
use bootloader_api::{BootloaderConfig,BootInfo};

use crate::bga::{VBE_DISPI_INDEX_VIRT_WIDTH, bga_read_register};

mod acpi;
mod memory;
//mod virtualization;

mod apic;
mod bga;
mod console;
mod context;
mod gdt;
mod interrupts;
mod logging;
mod pcie;
mod process;
mod qemu;
mod scheduler;
mod serial;
mod resources;

extern crate alloc;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = bootloader_api::BootloaderConfig::new_default();
    config.kernel_stack_size = 512 * 1024;
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main,config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    //crate::logging::init();
    crate::console::init(boot_info.framebuffer.as_mut().unwrap());

    crate::gdt::init();
    crate::interrupts::init();
    info!("GDT and IDT initialized");

    let boot_info = crate::memory::init(boot_info);
    info!("Memory initialized");

    crate::acpi::init(boot_info.rsdp_addr.as_ref().unwrap());
    info!("ACPI Initialized");

    //crate::process::init();
    //crate::scheduler::init();

    crate::pcie::init();
    info!("PCIe Initialized");

    crate::interrupts::enable_interrupts();
    info!("Interrupts enabled");

    println!("Hello from the x86_64 kernel!");
    println!("More debug info will be display in the serial console.");
    println!("Press Enter to poweroff.");
    serial_println!(
        "If you can't see more content here, you need to specify LOG_LEVEL env at compile time to enable higher level log filtering."
    );

    info!("Hello from the x86_64 kernel!");
    info!("This is the last message from the kernel.");

    let logo = png_decoder::decode(crate::resources::LOGO).unwrap();
    let img_width = logo.0.width;
    let img_height = logo.0.height;
    let pixels = logo.1;

    bga_set_video_mode(img_width, img_height, VBE_DISPI_BPP_32 as u32, true, true);
    bga_set_bank(0);

    // After bga_set_video_mode, the framebuffer stride equals the new width (img_width),
    // not the bootloader's original stride
    let framebuffer = boot_info.framebuffer.as_ref().unwrap().buffer().as_ptr() as *mut u32;
    unsafe {
        for y in 0..img_height {
            for x in 0..img_width {
                // source index uses image width
                let src_index = (y * img_width + x) as usize;
                // destination offset uses image width as stride (set by BGA mode)
                let dst_offset = (y as usize * img_width as usize) + x as usize;
                let pixel = pixels[src_index];
                let r = pixel[0];
                let g = pixel[1];
                let b = pixel[2];
                let a = pixel[3];
                let color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                let virt_width = bga_read_register(VBE_DISPI_INDEX_VIRT_WIDTH) as u32;
                let pitch = virt_width *4;
                let offset = y*pitch/4+x;
                if a > 0 {
                    //*framebuffer.add(dst_offset) = color;
                    *framebuffer.offset(offset as isize) = color;
                } else {
                    //*framebuffer.add(dst_offset) = 0x00000000;
                    *framebuffer.offset(src_index as isize) = 0x00000000;
                }
            }
        }
    }

    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}
