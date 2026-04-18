#![no_std]
#![no_main]
#![panic_handler]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

use core::panic::PanicInfo;

use bootloader_api::entry_point;
use kernel::bga::{VBE_DISPI_BPP_32, bga_set_bank, bga_set_video_mode};
use log::*;

use bootloader_api::{BootInfo, BootloaderConfig};

use kernel::bga::{VBE_DISPI_INDEX_VIRT_WIDTH, bga_read_register};

//mod virtualization;

extern crate alloc;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = bootloader_api::BootloaderConfig::new_default();
    config.kernel_stack_size = 512 * 1024;
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    //crate::logging::init();
    kernel::console::init(boot_info.framebuffer.as_mut().unwrap());

    kernel::gdt::init();
    kernel::interrupts::init();
    info!("GDT and IDT initialized");

    let boot_info = kernel::memory::init(boot_info);
    info!("Memory initialized");

    kernel::acpi::init(boot_info.rsdp_addr.as_ref().unwrap());
    info!("ACPI Initialized");

    kernel::apic::init(boot_info.rsdp_addr.as_ref().unwrap());
    kernel::mouse::init();
    //crate::process::init();
    //crate::scheduler::init();

    kernel::pcie::init();
    info!("PCIe Initialized");

    kernel::interrupts::enable_interrupts();
    info!("Interrupts enabled");

    info!("Hello from the x86_64 kernel!");
    info!("This is the last message from the kernel.");

    // After bga_set_video_mode, the framebuffer stride equals the new width (img_width),
    // not the bootloader's original stride
    let graph_flag: bool = false;
    if graph_flag == true {
        use kernel::console;
        let mut console_lock = console::CONSOLE.lock();
        if let Some(display) = console_lock.as_mut() {
            console::draw_desktop(display);
            let mut wm = console::WINDOW_MANAGER.lock();
            wm.create_window(display, 400, 300, "terminal");
            wm.create_window(display, 400, 300, "settings");
            //wm.open_window(display, 1);

            let mut prev_left = false;
            let mut prev_right = false;
            loop {
                let mouse_state = kernel::mouse::get_state();
                // 2. 业务逻辑：判断按键的“状态跃迁”（边缘触发）
                if mouse_state.left {// 按下左键后，根据上次左键状态进行修改
                    let mut wm = console::WINDOW_MANAGER.lock();
                    // if prev_left == true {
                    //     prev_left = false;
                    //     wm.close_window(display, 1);
                    // }
                    // else {
                    //     prev_left = true;
                    //     wm.open_window(display, 1);
                    // }
                    wm.open_window(display, 1);
                }
                else {
                    let mut wm = console::WINDOW_MANAGER.lock();
                    wm.open_window(display, 1);
                }
                if mouse_state.right {//右键同理
                    let mut wm = console::WINDOW_MANAGER.lock();
                    if prev_right == true {
                        prev_right = false;
                        wm.close_window(display, 2);
                    }
                    else {
                        prev_right = true;
                        wm.open_window(display, 2);
                    }
                }
                
                display.draw_mouse(mouse_state.x, mouse_state.y);

                x86_64::instructions::hlt();
            }
        }
    } else {
        kernel::println!("Hello from the Rix-OS kernel!");
        kernel::println!("More debug info will be display in the serial console.");
        kernel::println!("Press Enter to poweroff.");
        kernel::serial_println!(
            "If you can't see more content here, you need to specify LOG_LEVEL env at compile time to enable higher level log filtering."
        );
        kernel::println!("It's kernel: txt mode");
        use alloc::{boxed::Box, vec::Vec};
        let heap_value = Box::new(41);
        kernel::println!("heap_value at {:p}", heap_value);

        let start_t = unsafe { core::arch::x86_64::_rdtsc() };
        let mut vec = Vec::new();
        for i in 0..500 {
            vec.push(i);
        }

        let end_t = unsafe { core::arch::x86_64::_rdtsc() };
        let margin = end_t - start_t;
        kernel::println!("Allocated 500 i32 in {} CPU cycles", margin);
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
