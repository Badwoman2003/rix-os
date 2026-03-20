use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;

// VGA图形模式常量
pub const VGA_WIDTH: usize = 320;
pub const VGA_HEIGHT: usize = 200;
const VGA_BUFFER_ADDRESS: usize = 0xA0000;

lazy_static! {
    /// 全局图形写入器实例
    pub static ref WRITER: Mutex<GraphicsWriter> = Mutex::new(GraphicsWriter::new());
}

/// VGA调色板颜色
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// 图形模式的VGA写入器
pub struct GraphicsWriter {
    x_pos: usize,
    y_pos: usize,
    color: Color,
    buffer: &'static mut [u8],
}

impl GraphicsWriter {
    /// 创建新的图形写入器
    const fn new() -> Self {
        Self {
            x_pos: 0,
            y_pos: 0,
            color: Color::White,
            buffer: unsafe { 
                core::slice::from_raw_parts_mut(VGA_BUFFER_ADDRESS as *mut u8, VGA_WIDTH * VGA_HEIGHT)
            },
        }
    }

    /// 初始化VGA图形模式 (Mode 13h: 320x200x256)
    pub fn init_graphics_mode(&mut self) {
        unsafe {
            // 设置VGA寄存器切换到Mode 13h
            let mut misc_port = Port::<u8>::new(0x3C2);
            misc_port.write(0x63);

            // 序列器寄存器
            let mut seq_addr = Port::<u8>::new(0x3C4);
            let mut seq_data = Port::<u8>::new(0x3C5);
            
            seq_addr.write(0x00); seq_data.write(0x03);
            seq_addr.write(0x01); seq_data.write(0x01);
            seq_addr.write(0x02); seq_data.write(0x0F);
            seq_addr.write(0x03); seq_data.write(0x00);
            seq_addr.write(0x04); seq_data.write(0x0E);

            // CRTC寄存器
            let mut crtc_addr = Port::<u8>::new(0x3D4);
            let mut crtc_data = Port::<u8>::new(0x3D5);
            
            let crtc_values = [
                0x5F, 0x4F, 0x50, 0x82, 0x54, 0x80, 0xBF, 0x1F,
                0x00, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x9C, 0x0E, 0x8F, 0x28, 0x40, 0x96, 0xB9, 0xA3,
                0xFF
            ];
            
            for (i, &value) in crtc_values.iter().enumerate() {
                crtc_addr.write(i as u8);
                crtc_data.write(value);
            }

            // 图形寄存器
            let mut gfx_addr = Port::<u8>::new(0x3CE);
            let mut gfx_data = Port::<u8>::new(0x3CF);
            
            gfx_addr.write(0x00); gfx_data.write(0x00);
            gfx_addr.write(0x01); gfx_data.write(0x00);
            gfx_addr.write(0x02); gfx_data.write(0x00);
            gfx_addr.write(0x03); gfx_data.write(0x00);
            gfx_addr.write(0x04); gfx_data.write(0x00);
            gfx_addr.write(0x05); gfx_data.write(0x40);
            gfx_addr.write(0x06); gfx_data.write(0x05);
            gfx_addr.write(0x07); gfx_data.write(0x0F);
            gfx_addr.write(0x08); gfx_data.write(0xFF);

            // 属性寄存器
            let mut attr_addr = Port::<u8>::new(0x3C0);
            let mut input_status = Port::<u8>::new(0x3DA);
            
            // 读取输入状态以重置属性寄存器状态
            input_status.read();
            
            let attr_values = [
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
                0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                0x41, 0x00, 0x0F, 0x00, 0x00
            ];
            
            for (i, &value) in attr_values.iter().enumerate() {
                attr_addr.write(i as u8);
                attr_addr.write(value);
            }
            
            attr_addr.write(0x20); // 启用视频
        }
        
        // 清屏
        self.clear_screen(Color::Black);
    }

    /// 设置像素颜色
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x < VGA_WIDTH && y < VGA_HEIGHT {
            let offset = y * VGA_WIDTH + x;
            self.buffer[offset] = color as u8;
        }
    }

    /// 清屏
    pub fn clear_screen(&mut self, color: Color) {
        for pixel in self.buffer.iter_mut() {
            *pixel = color as u8;
        }
        self.x_pos = 0;
        self.y_pos = 0;
    }

    /// 绘制字符 (简化的8x8字体)
    pub fn draw_char(&mut self, ch: char, x: usize, y: usize, color: Color) {
        let font_data = get_font_data(ch);
        
        for (row_idx, &byte) in font_data.iter().enumerate() {
            for col_idx in 0..8 {
                if byte & (0x80 >> col_idx) != 0 {
                    // 修正：字体数据是旋转180度存储的，所以我们需要在绘制时翻转回来
                    let x_pos = x + (7 - col_idx);
                    self.set_pixel(x_pos, y + row_idx, color);
                }
            }
        }
    }

    /// 在指定坐标绘制字符串
    pub fn draw_string(&mut self, s: &str, x: usize, y: usize, color: Color) {
        let mut current_x = x;
        for ch in s.chars() {
            self.draw_char(ch, current_x, y, color);
            current_x += 8; // 每个字符宽度为8像素
        }
    }

    /// 写入字节
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\r' => self.x_pos = 0,
            byte => {
                if self.x_pos >= VGA_WIDTH / 8 {
                    self.new_line();
                }

                self.draw_char(byte as char, self.x_pos * 8, self.y_pos * 8, self.color);
                self.x_pos += 1;
            }
        }
    }

    /// 写入字符串
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // 可打印ASCII字符或换行符
                0x20..=0x7e | b'\n' | b'\r' => self.write_byte(byte),
                // 非ASCII字符用'?'表示
                _ => self.write_byte(b'?'),
            }
        }
    }

    /// 换行
    fn new_line(&mut self) {
        self.y_pos += 1;
        self.x_pos = 0;

        // 如果超出屏幕，向上滚动
        if self.y_pos >= VGA_HEIGHT / 8 {
            self.scroll_up();
            self.y_pos = VGA_HEIGHT / 8 - 1;
        }
    }

    /// 向上滚动屏幕
    fn scroll_up(&mut self) {
        // 向上滚动8个像素（一个字符行的高度）
        let scroll_amount = 8 * VGA_WIDTH;
        self.buffer.copy_within(scroll_amount.., 0);

        // 清除屏幕底部的最后8行
        let clear_start = (VGA_HEIGHT - 8) * VGA_WIDTH;
        self.buffer[clear_start..].fill(Color::Black as u8);
    }

    /// 设置文本颜色
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    /// 绘制线条（使用Bresenham算法）
    pub fn draw_line(&mut self, x1: usize, y1: usize, x2: usize, y2: usize, color: Color) {
        let dx = if x2 > x1 { x2 - x1 } else { x1 - x2 } as isize;
        let dy = -(if y2 > y1 { y2 - y1 } else { y1 - y2 } as isize);
        let sx = if x1 < x2 { 1isize } else { -1isize };
        let sy = if y1 < y2 { 1isize } else { -1isize };
        let mut err = (if dx > -dy { dx } else { dy }) / 2;

        let mut x = x1 as isize;
        let mut y = y1 as isize;
        let x2 = x2 as isize;
        let y2 = y2 as isize;

        loop {
            // 确保坐标在有效范围内
            if x >= 0 && y >= 0 && (x as usize) < VGA_WIDTH && (y as usize) < VGA_HEIGHT {
                self.set_pixel(x as usize, y as usize, color);
            }
            
            if x == x2 && y == y2 {
                break;
            }
            
            let e2 = err;
            if e2 > dy {
                err += dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// 绘制矩形
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
    }
}

impl fmt::Write for GraphicsWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// 获取字符的字体数据
fn get_font_data(ch: char) -> [u8; 8] {
    match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x18, 0x3C, 0x3C, 0x18, 0x18, 0x00, 0x18, 0x00],
        '"' => [0x36, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '#' => [0x36, 0x36, 0x7F, 0x36, 0x7F, 0x36, 0x36, 0x00],
        '(' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        ')' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        '/' => [0x60, 0x30, 0x18, 0x0C, 0x06, 0x03, 0x01, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x1C, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x60, 0x30, 0x18, 0x0C, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x60, 0x38, 0x60, 0x66, 0x3C, 0x00],
        '4' => [0x60, 0x70, 0x78, 0x6C, 0x66, 0x7E, 0x60, 0x00],
        '5' => [0x7E, 0x06, 0x3E, 0x60, 0x60, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x66, 0x06, 0x3E, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x66, 0x30, 0x18, 0x18, 0x18, 0x18, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x7C, 0x60, 0x66, 0x3C, 0x00],
        ':' => [0x00, 0x00, 0x18, 0x00, 0x00, 0x18, 0x00, 0x00],
        'A' => [0x18, 0x3C, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'B' => [0x3E, 0x66, 0x66, 0x3E, 0x66, 0x66, 0x3E, 0x00],
        'C' => [0x3C, 0x66, 0x06, 0x06, 0x06, 0x66, 0x3C, 0x00],
        'D' => [0x1E, 0x36, 0x66, 0x66, 0x66, 0x36, 0x1E, 0x00],
        'E' => [0x7E, 0x06, 0x06, 0x3E, 0x06, 0x06, 0x7E, 0x00],
        'F' => [0x7E, 0x06, 0x06, 0x3E, 0x06, 0x06, 0x06, 0x00],
        'G' => [0x3C, 0x66, 0x06, 0x76, 0x66, 0x66, 0x3C, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'L' => [0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x6E, 0x7E, 0x76, 0x66, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06, 0x06, 0x00],
        'R' => [0x3E, 0x66, 0x66, 0x3E, 0x1E, 0x36, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x06, 0x3C, 0x60, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x60, 0x7C, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x06, 0x06, 0x06, 0x3C, 0x00],
        'd' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x06, 0x3C, 0x00],
        'g' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x3E],
        'h' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x1C, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'k' => [0x06, 0x06, 0x66, 0x36, 0x1E, 0x36, 0x66, 0x00],
        'l' => [0x1C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x33, 0x7F, 0x7F, 0x6B, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'r' => [0x00, 0x00, 0x3E, 0x66, 0x06, 0x06, 0x06, 0x00],
        's' => [0x00, 0x00, 0x7C, 0x06, 0x3C, 0x60, 0x3E, 0x00],
        't' => [0x0C, 0x0C, 0x3E, 0x0C, 0x0C, 0x0C, 0x38, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x7C, 0x60, 0x3E],
        // 默认字符
        _ => [0xFF, 0x81, 0x81, 0x81, 0x81, 0x81, 0xFF, 0x00],
    }
}