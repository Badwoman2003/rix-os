use log::{debug, warn};
use spin::MutexGuard;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use lazy_static::lazy_static;

use crate::apic::LAPIC;
use crate::{println, serial_println};

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = 32,
    Keyboard = 33,
    Mouse = 44,
    LapicError = 51,
    Spurious = 0xff,
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

macro_rules! simple_handlers {
    ($($name:ident => $info:expr),* $(,)?) => {
        $(
            pub extern "x86-interrupt" fn $name(
                stack_frame: InterruptStackFrame
            ) {
                serial_println!("EXCEPTION: {}\n{:#?}", $info, stack_frame);
            }
        )*
    };
}

macro_rules! error_code_handlers {
    ($($name:ident => $info:expr),* $(,)?) => {
        $(
            pub extern "x86-interrupt" fn $name(
                stack_frame: InterruptStackFrame,
                error_code: u64
            ) {
                serial_println!(
                    "EXCEPTION: {} - ERROR CODE: {}\n{:#?}",
                    $info, error_code, stack_frame
                );
            }
        )*
    };
}

#[rustfmt::skip]
const SIMPLE_HANDLERS: &[(
    fn(&mut InterruptDescriptorTable) -> &mut x86_64::structures::idt::Entry<extern "x86-interrupt" fn(InterruptStackFrame)>,
    extern "x86-interrupt" fn(InterruptStackFrame),
)] = &[
    (|idt| &mut idt.divide_error, divide_by_zero_handler),
    (|idt| &mut idt.debug, debug_handler),
    (|idt| &mut idt.non_maskable_interrupt, non_maskable_interrupt_handler),
    (|idt| &mut idt.overflow, overflow_handler),
    (|idt| &mut idt.bound_range_exceeded, bound_range_exceeded_handler),
    (|idt| &mut idt.invalid_opcode, invalid_opcode_handler),
    (|idt| &mut idt.device_not_available, device_not_available_handler),
    (|idt| &mut idt.x87_floating_point, x87_floating_point_handler),
    (|idt| &mut idt.simd_floating_point, simd_floating_point_handler),
    (|idt| &mut idt.virtualization, virtualization_handler),
    (|idt| &mut idt.breakpoint, breakpoint_handler),
];

#[rustfmt::skip]
const ERROR_CODE_HANDLERS: &[(
    fn(&mut InterruptDescriptorTable) -> &mut x86_64::structures::idt::Entry<extern "x86-interrupt" fn(InterruptStackFrame, u64)>,
    extern "x86-interrupt" fn(InterruptStackFrame, u64),
)] = &[
    (|idt| &mut idt.invalid_tss, invalid_tss_handler),
    (|idt| &mut idt.segment_not_present, segment_not_present_handler),
    (|idt| &mut idt.stack_segment_fault, stack_segment_fault_handler),
    (|idt| &mut idt.general_protection_fault, general_protection_fault_handler),
    (|idt| &mut idt.alignment_check, alignment_check_handler),
    (|idt| &mut idt.security_exception, security_exception_handler),
];

simple_handlers!(
    divide_by_zero_handler          => "DIVIDE BY ZERO",
    debug_handler                   => "DEBUG",
    non_maskable_interrupt_handler  => "NON MASKABLE INTERRUPT",
    overflow_handler                => "OVERFLOW",
    bound_range_exceeded_handler    => "BOUND RANGE EXCEEDED",
    invalid_opcode_handler          => "INVALID OPCODE",
    device_not_available_handler    => "DEVICE NOT AVAILABLE",
    x87_floating_point_handler      => "X87 FLOATING POINT",
    simd_floating_point_handler     => "SIMD FLOATING POINT",
    virtualization_handler          => "VIRTUALIZATION",
);

error_code_handlers!(
    invalid_tss_handler                 => "INVALID TSS",
    segment_not_present_handler         => "SEGMENT NOT PRESENT",
    stack_segment_fault_handler         => "STACK SEGMENT FAULT",
    general_protection_fault_handler    => "GENERAL PROTECTION FAULT",
    alignment_check_handler             => "ALIGNMENT CHECK",
    security_exception_handler          => "SECURITY EXCEPTION",
);

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        for (getter, handler) in SIMPLE_HANDLERS {
            getter(&mut idt).set_handler_fn(*handler);
        }

        for (getter, handler) in ERROR_CODE_HANDLERS {
            getter(&mut idt).set_handler_fn(*handler);
        }

        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        }

        #[rustfmt::skip]
        idt.machine_check.set_handler_fn(machine_check_handler);
        #[rustfmt::skip]
        idt.page_fault.set_handler_fn(page_fault_handler);

        #[rustfmt::skip]
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        #[rustfmt::skip]
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        #[rustfmt::skip]
        idt[InterruptIndex::Mouse.as_u8()].set_handler_fn(mouse_interrupt_handler);
        #[rustfmt::skip]
        idt[InterruptIndex::LapicError.as_u8()].set_handler_fn(lapic_error_handler);
        #[rustfmt::skip]
        idt[InterruptIndex::Spurious.as_u8()].set_handler_fn(spurious_interrupt_handler);

        idt
    };
}

pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    serial_println!(
        "EXCEPTION: DOUBLE FAULT - ERROR CODE: {}\n{:#?}",
        error_code,
        stack_frame
    );
    loop {
        x86_64::instructions::hlt();
    }
}

pub extern "x86-interrupt" fn machine_check_handler(stack_frame: InterruptStackFrame) -> ! {
    serial_println!("EXCEPTION: MACHINE CHECK\n{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

pub extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    serial_println!(
        "EXCEPTION: PAGE FAULT - ERROR CODE: {:?}\nAccessed Address: {:?}\n{:#?}",
        error_code,
        Cr2::read(),
        stack_frame
    );
}

pub extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);

    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

use core::sync::atomic::{AtomicU64, Ordering};

static TSC_COUNT: AtomicU64 = AtomicU64::new(0);
pub extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use crate::apic::LAPIC;

    // EOI must be sent before context_switch, because the switch may not
    // return to this handler until this task is scheduled again.
    let curr_tsc = TSC_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if curr_tsc % 100 == 0 {
        println!("{} ticks!", curr_tsc);
    }

    unsafe {
        #[allow(static_mut_refs)]
        LAPIC.get().unwrap().lock().end_interrupts();
    }

    //crate::scheduler::tick();
}

use spin::Mutex;

pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use crate::{print, println};
    use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
    use spin::Mutex;
    use x86_64::instructions::port::Port;

    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
            Mutex::new(Keyboard::new(
                ScancodeSet1::new(),
                layouts::Us104Key,
                HandleControl::Ignore
            ));
    }

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    print!("{}", character)
                }
                DecodedKey::RawKey(key) => {}
            }
        }
    }

    warn!("Keyboard scancode: {}", scancode);
    if scancode == 28 {
        crate::acpi::shutdown();
    }

    unsafe {
        #[allow(static_mut_refs)]
        LAPIC.get().unwrap().lock().end_interrupts();
    }
}

pub extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use spin::Mutex;
    use x86_64::instructions::port::Port;
    use crate::mouse;

    // 使用静态变量收集 3 字节数据包
    lazy_static! {
        static ref PACKET: Mutex<([u8; 3], usize)> = Mutex::new(([0u8; 3], 0));
    }

    let mut port = Port::<u8>::new(0x60);
    let byte: u8 = unsafe { port.read() };

    // 忽略初始化期间鼠标返回的 ACK/BAT 字节
    if byte == 0xFA || byte == 0xAA {
        unsafe {
            #[allow(static_mut_refs)]
            LAPIC.get().unwrap().lock().end_interrupts();
        }
        return;
    }

    let mut guard = PACKET.lock();
    let (ref mut data, ref mut index) = *guard;

    // 首字节同步：mouse 数据包第一字节始终 bit3 = 1。
    if *index == 0 && (byte & 0x08) == 0 {
        // 非有效首字节，忽略当前字节以重新同步
        unsafe {
            #[allow(static_mut_refs)]
            LAPIC.get().unwrap().lock().end_interrupts();
        }
        return;
    }

    data[*index] = byte;
    *index += 1;

    if *index == 3 {
        // 把数据包交给 mouse 模块处理
        mouse::handle_packet(*data);
        
        // 可根据需要打印调试信息
        // crate::println!("mouse packet: {:02x?}", data);

        *index = 0;
    }

    unsafe {
        #[allow(static_mut_refs)]
        LAPIC.get().unwrap().lock().end_interrupts();
    }
}

pub extern "x86-interrupt" fn lapic_error_handler(_stack_frame: InterruptStackFrame) {
    use crate::apic::LAPIC;

    warn!("LAPIC ERROR interrupt received");

    // Must send EOI for LAPIC error interrupt
    unsafe {
        #[allow(static_mut_refs)]
        LAPIC.get().unwrap().lock().end_interrupts();
    }
}

pub extern "x86-interrupt" fn spurious_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Spurious interrupts should NOT send EOI
    debug!("Spurious interrupt received");
}

pub fn init() {
    IDT.load();
}

pub fn enable_interrupts() {
    debug!("Enabling interrupts");
    x86_64::instructions::interrupts::enable();
    debug!("Interrupts enabled");
}
