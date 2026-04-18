#![allow(dead_code)]

use x86_64::instructions::port::Port;
use lazy_static::lazy_static;
use spin::Mutex;

use crate::println;

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64; // 也用作命令端口

// PS/2 控制器状态位
// bit 0: Output buffer status (1=full)
// bit 1: Input buffer status (1=full)

fn wait_read() {
    // 等待输出缓冲区有数据可读
    while unsafe { Port::<u8>::new(PS2_STATUS_PORT).read() } & 1 == 0 {}
}

fn wait_write() {
    // 等待输入缓冲区可写
    while unsafe { Port::<u8>::new(PS2_STATUS_PORT).read() } & 2 != 0 {}
}

pub fn init() {
    unsafe {
        let mut cmd = Port::<u8>::new(PS2_STATUS_PORT);
        let mut data = Port::<u8>::new(PS2_DATA_PORT);

        // 1. 启用第二 PS/2 端口（鼠标）
        wait_write();
        cmd.write(0xA8); // enable second port

        // 2. 读取现有配置字节
        wait_write();
        cmd.write(0x20);
        wait_read();
        let mut config: u8 = data.read();

        // 3. 设置使能鼠标中断（位 1）
        config |= 0x02; // bit1 = 1 使能 IRQ12
        //    同时保持键盘中断（位0）状态

        // 4. 写回配置字节
        wait_write();
        cmd.write(0x60); // 准备写配置字节
        wait_write();
        data.write(config);

        // 5. 发送"启用数据报告"命令给鼠标 (0xF4)
        //    需先告诉控制器接下来命令发送到鼠标 (0xD4)
        wait_write();
        cmd.write(0xD4);
        wait_write();
        data.write(0xF4);
        // 可忽略 ACK
    }
}

/// 当前鼠标状态（屏幕绝对坐标 + 按键）
#[derive(Clone, Copy, Debug)]
pub struct MouseState {
    pub x: usize,
    pub y: usize,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub released: bool,
}

// 默认屏幕分辨率，与 vga_buffer 常量保持一致
const SCREEN_WIDTH: usize = 320;
const SCREEN_HEIGHT: usize = 200;

lazy_static! {
    static ref STATE: Mutex<MouseState> = Mutex::new(MouseState {
        x: SCREEN_WIDTH / 2,
        y: SCREEN_HEIGHT / 2,
        left: false,
        right: false,
        middle: false,
        released: false,
    });
}

lazy_static! {
    static ref LEFT_ST: Mutex<bool> = Mutex::new(false);
}

lazy_static! {
    static ref RIGHT_ST: Mutex<bool> = Mutex::new(false);
}

/// 内核其他模块调用，用于获取最新的鼠标状态
pub fn get_state() -> MouseState {
    *STATE.lock()
}

/// 同上
pub fn get_buttom_state()->(bool,bool) {
    (*LEFT_ST.lock(),*RIGHT_ST.lock())
}


/// 由中断处理程序调用，更新鼠标状态
pub fn handle_packet(packet: [u8; 3]) {
    // packet[0] = buttons & flags
    // packet[1] = dx (i8)
    // packet[2] = dy (i8) —— 向上为负
    let mut state = STATE.lock();

    let dx = packet[1] as i8 as isize;
    let dy = packet[2] as i8 as isize;

    // PS/2 Y 轴正向向上，屏幕 Y 轴向下增长，需要取反
    let new_x = state.x as isize + dx;
    let new_y = state.y as isize - dy;

    state.x = new_x.clamp(0, (SCREEN_WIDTH - 1) as isize) as usize;
    state.y = new_y.clamp(0, (SCREEN_HEIGHT - 1) as isize) as usize;

    
    let p_l_state = packet[0];
    match p_l_state {
        0x09 => state.left = true,
        0x0a => state.right = true,
        0x04 => state.middle = true,
        0x08 => state.released = true,
        _ => {}
    }

    if state.left {
        // let mut left_st = *LEFT_ST.lock();
        // if left_st {
        //     left_st = false;
        // }
        // else {
        //     left_st = true;
        // }
        println!("left pressed!")
    }
    if state.right {
        // let mut  right_st = *RIGHT_ST.lock();
        // if right_st {
        //     right_st = false;
        // }
        // else {
        //     right_st = true;
        // }
        println!("right pressed!")
    }
    // if state.middle {
    //     crate::println!("middle button pressed");
    // }
    // if state.released {
    //     crate::println!("released");
    // }
    crate::println!("x: {}, y: {}",state.x,state.y);

    // 更新状态后，将状态重置为未按下
    state.left = false;
    state.right = false;
    state.middle = false;
    state.released = false;
} 