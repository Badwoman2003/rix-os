use core::fmt::Write;

use bootloader_api::info::FrameBuffer;

use lazy_static::lazy_static;
use nostd::string::String;
use nostd::vec::Vec;
use noto_sans_mono_bitmap::FontWeight;
use noto_sans_mono_bitmap::RasterHeight;
use noto_sans_mono_bitmap::get_raster;
use noto_sans_mono_bitmap::get_raster_width;
use spin::Mutex;
use x86_64::instructions::interrupts;
extern crate alloc;
use alloc::string::ToString;

use crate::bga;

lazy_static! {
    pub static ref CONSOLE: Mutex<Option<NotoFontDisplay>> = Mutex::new(None);
}

// 鼠标光标的尺寸
const MOUSE_WIDTH: usize = 11;
const MOUSE_HEIGHT: usize = 16;

// 简单的箭头光标掩码：0=透明，1=黑色边框，2=白色填充
const MOUSE_BITMAP: [[u8; MOUSE_WIDTH]; MOUSE_HEIGHT] = [
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 1, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 2, 1, 0],
    [1, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1],
    [1, 2, 2, 1, 2, 2, 1, 0, 0, 0, 0],
    [1, 2, 1, 0, 1, 2, 2, 1, 0, 0, 0],
    [1, 1, 0, 0, 1, 2, 2, 1, 0, 0, 0],
    [1, 0, 0, 0, 0, 1, 2, 2, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0],
];

pub struct NotoFontDisplay {
    width: usize,
    height: usize,
    // pixels per row in memory (may include alignment padding)
    stride: usize,
    draw_buffer: &'static mut [u32],

    font_weight: FontWeight,
    raster_height: RasterHeight,

    cursor_x: usize,
    cursor_y: usize,

    // 新增：用于实现不破坏背景的鼠标状态
    mouse_saved_bg: [u32; MOUSE_WIDTH * MOUSE_HEIGHT],
    old_mouse_x: usize,
    old_mouse_y: usize,
    mouse_drawn_once: bool,
}

pub fn init(frame_buffer: &mut FrameBuffer) {
    let buffer = frame_buffer.buffer_mut().as_ptr() as *mut u32;
    let width = frame_buffer.info().width;
    let height = frame_buffer.info().height;
    // use stride instead of width for memory calculation
    let stride = frame_buffer.info().stride;

    // use stride * height as buffer size
    for index in 0..(stride * height) {
        unsafe {
            buffer.add(index as usize).write(0xff408deb);
        }
    }

    let mut console = NotoFontDisplay::new(
        width as usize,
        height as usize,
        stride as usize,
        unsafe { core::slice::from_raw_parts_mut(buffer, (stride * height) as usize) },
        FontWeight::Light,
        RasterHeight::Size24,
    );
    console.clear();

    interrupts::without_interrupts(|| {
        CONSOLE.lock().replace(console);

        // CONSOLE
        //     .lock()
        //     .as_mut()
        //     .unwrap()
        //     .draw_string("Kernel Message");
    });
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        if let Some(console) = CONSOLE.lock().as_mut() {
            console.write_fmt(args).expect("Printing to console failed");
        }
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(
        concat!($fmt, "\n"), $($arg)*));
}

impl NotoFontDisplay {
    pub fn new(
        width: usize,
        height: usize,
        stride: usize,
        draw_buffer: &'static mut [u32],
        font_weight: FontWeight,
        raster_height: RasterHeight,
    ) -> Self {
        Self {
            width,
            height,
            stride,
            draw_buffer,
            font_weight,
            raster_height,
            cursor_x: 0,
            cursor_y: 0,

            //鼠标相关
            mouse_saved_bg: [0u32; MOUSE_WIDTH * MOUSE_HEIGHT],
            old_mouse_x: width / 2,
            old_mouse_y: height / 2,
            mouse_drawn_once: false,
        }
    }

    pub fn clear(&mut self) {
        for pixel in self.draw_buffer.iter_mut() {
            *pixel = 0;
        }
    }

    pub fn draw_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            let index = y * self.stride + x;
            self.draw_buffer[index] = color;
        }
    }

    pub fn draw_string(&mut self, msg: &str) {
        for c in msg.chars() {
            // 1. 手动处理换行符 (println! 传进来的就是 \n)
            if c == '\n' {
                self.cursor_x = 0;
                self.cursor_y += self.raster_height as usize;
                continue;
            }

            let char_raster = match get_raster(c, self.font_weight, self.raster_height) {
                Some(raster) => raster,
                None => get_raster(' ', self.font_weight, self.raster_height).unwrap(),
            };

            // 2. 检查是否需要自动换行
            if self.cursor_x + char_raster.width() > self.width {
                self.cursor_x = 0;
                self.cursor_y += self.raster_height as usize;
            }

            // 3. 纵向越界保护：如果光标超出了屏幕底部，清屏并回到原点
            // （后续你可以将其升级为画面整体上移的 Scroll 逻辑）
            if self.cursor_y + (self.raster_height as usize) > self.height {
                self.clear();
                self.cursor_x = 0;
                self.cursor_y = 0;
            }

            // 4. 绘制单个字符
            for (row_i, row) in char_raster.raster().iter().enumerate() {
                for (col_i, intensity) in row.iter().enumerate() {
                    if *intensity == 0 {
                        continue;
                    } // 优化：完全透明的像素直接跳过

                    // 计算物理内存位置，注意这里必须全程使用 stride 计算 Y 偏移
                    let index = (self.cursor_y + row_i) * self.stride + (self.cursor_x + col_i);

                    // 最后一道防线：确保绝对不越界
                    if index < self.draw_buffer.len() {
                        let curr_pixel_rgb = self.draw_buffer[index];
                        let mut r = ((curr_pixel_rgb & 0xff0000) >> 16) as u8;
                        let mut g = ((curr_pixel_rgb & 0xff00) >> 8) as u8;
                        let mut b = (curr_pixel_rgb & 0xff) as u8;

                        r = r.saturating_add(*intensity);
                        g = g.saturating_add(*intensity);
                        b = b.saturating_add(*intensity);

                        let new_pixel_rgb = ((r as u32) << 16) + ((g as u32) << 8) + (b as u32);
                        self.draw_buffer[index] = new_pixel_rgb;
                    }
                }
            }

            // 5. 字符画完后，光标仅仅横向向右移动该字符的宽度！
            self.cursor_x += char_raster.width();
        }
    }

    pub fn draw_string_at(&mut self, msg: &str, x: usize, y: usize, color: u32) {
        let mut current_x = x;
        for c in msg.chars() {
            if let Some(char_raster) = get_raster(c, self.font_weight, self.raster_height) {
                for (row_i, row) in char_raster.raster().iter().enumerate() {
                    for (col_i, &intensity) in row.iter().enumerate() {
                        if intensity > 0 {
                            self.draw_pixel(current_x + col_i, y + row_i, color);
                        }
                    }
                }
                current_x += char_raster.width();
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: isize, mut y0: isize, x1: isize, y1: isize, color: u32) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            // 确保坐标在屏幕范围内
            if x0 >= 0 && y0 >= 0 && (x0 as usize) < self.width && (y0 as usize) < self.height {
                self.draw_pixel(x0 as usize, y0 as usize, color);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// 任务2：基于直线实现矩形边框绘制
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let x0 = x as isize;
        let y0 = y as isize;
        let x1 = (x + width - 1) as isize;
        let y1 = (y + height - 1) as isize;

        self.draw_line(x0, y0, x1, y0, color); // 上边
        self.draw_line(x1, y0, x1, y1, color); // 右边
        self.draw_line(x1, y1, x0, y1, color); // 下边
        self.draw_line(x0, y1, x0, y0, color); // 左边
    }

    /// 辅助功能：填充矩形（用于窗口背景和任务栏）
    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: u32) {
        for cy in y..y.saturating_add(height).min(self.height) {
            for cx in x..x.saturating_add(width).min(self.width) {
                self.draw_pixel(cx, cy, color);
            }
        }
    }

    pub fn draw_image(
        &mut self,
        x: usize,
        y: usize,
        image: (png_decoder::PngHeader, Vec<[u8; 4]>),
    ) {
        let img_width = image.0.width;
        let img_height = image.0.height;
        let pixels = image.1;
        unsafe {
            for cy in 0..img_height {
                for cx in 0..img_width {
                    let src_index = (cy * img_width + cx) as usize;
                    let pixel: [u8; 4] = pixels[src_index];

                    let r_src = pixel[0] as u32;
                    let g_src = pixel[1] as u32;
                    let b_src = pixel[2] as u32;
                    let a_src = pixel[3] as u32;

                    // 1. 完全透明的像素直接跳过，提高性能
                    if a_src == 0 {
                        continue;
                    }

                    // 计算在屏幕（draw_buffer）上的绝对坐标
                    let screen_x = x as u32 + cx;
                    let screen_y = y as u32 + cy;
                    let color = ((r_src as u32) << 16) | ((g_src as u32) << 8) | (b_src as u32);
                    let virt_width = bga::bga_read_register(bga::VBE_DISPI_INDEX_VIRT_WIDTH) as u32;
                    let pitch = virt_width * 4;
                    let offset = (screen_y * pitch / 4 + screen_x) as usize;

                    if a_src > 0 {
                        self.draw_buffer[offset] = color;
                    }
                }
            }
        }
    }

    pub fn draw_mouse(&mut self, new_x: usize, new_y: usize) {
        // 1. 恢复上一次鼠标所在位置的背景
        if self.mouse_drawn_once {
            for cy in 0..MOUSE_HEIGHT {
                for cx in 0..MOUSE_WIDTH {
                    let screen_x = self.old_mouse_x + cx;
                    let screen_y = self.old_mouse_y + cy;

                    if screen_x < self.width && screen_y < self.height {
                        let dest_index = screen_y * self.stride + screen_x;
                        let bg_index = cy * MOUSE_WIDTH + cx;

                        if dest_index < self.draw_buffer.len() {
                            self.draw_buffer[dest_index] = self.mouse_saved_bg[bg_index];
                        }
                    }
                }
            }
        }

        // 2. 保存当前新位置的背景像素
        for cy in 0..MOUSE_HEIGHT {
            for cx in 0..MOUSE_WIDTH {
                let screen_x = new_x + cx;
                let screen_y = new_y + cy;

                if screen_x < self.width && screen_y < self.height {
                    let src_index = screen_y * self.stride + screen_x;
                    let bg_index = cy * MOUSE_WIDTH + cx;

                    if src_index < self.draw_buffer.len() {
                        self.mouse_saved_bg[bg_index] = self.draw_buffer[src_index];
                    }
                } else {
                    // 如果超出屏幕边界，用黑色填充保存区（防止越界取值）
                    let bg_index = cy * MOUSE_WIDTH + cx;
                    self.mouse_saved_bg[bg_index] = 0x000000;
                }
            }
        }

        // 3. 在新位置绘制鼠标光标
        for cy in 0..MOUSE_HEIGHT {
            for cx in 0..MOUSE_WIDTH {
                let screen_x = new_x + cx;
                let screen_y = new_y + cy;

                if screen_x < self.width && screen_y < self.height {
                    let dest_index = screen_y * self.stride + screen_x;

                    if dest_index < self.draw_buffer.len() {
                        let pixel_type = MOUSE_BITMAP[cy][cx];

                        // 1 为黑色边框，2 为白色填充，0 不绘制（保持透明）
                        if pixel_type == 1 {
                            self.draw_buffer[dest_index] = 0x000000; // 黑色
                        } else if pixel_type == 2 {
                            self.draw_buffer[dest_index] = 0xFFFFFF; // 白色
                        }
                    }
                }
            }
        }

        // 4. 更新坐标和状态
        self.old_mouse_x = new_x;
        self.old_mouse_y = new_y;
        self.mouse_drawn_once = true;
    }
}

impl Write for NotoFontDisplay {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        //self.draw_string_at(s,self.cursor_x,self.cursor_y,0xFFFFFF);
        self.draw_string(s);
        Ok(())
    }
}

/// 代表一个独立的窗口
pub struct Window {
    pub id: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    // 保存窗口覆盖区域的原始像素
    pub title:String,
    pub backup_buffer: Vec<u32>,
    pub open: bool,
}

/// 任务3：简单的窗口管理器
pub struct WindowManager {
    windows: Vec<Window>,
    next_id: usize,
}

impl WindowManager {
    pub const fn new() -> Self {
        Self {
            windows: Vec::new(),
            next_id: 1,
        }
    }

    /// 任务1：仅创建窗口数据对象，存入容器，但不显示
    pub fn create_window(
        &mut self,
        display: &mut NotoFontDisplay,
        width: usize,
        height: usize,
        title: &str,
    ) -> usize {
        let x = display.width.saturating_sub(width) / 2;
        let y = display.height.saturating_sub(height) / 2;
        let id = self.next_id;
        self.next_id += 1;

        self.windows.push(Window {
            id,
            x,
            y,
            width,
            height,
            title: title.to_string(), // 保存标题供后续渲染
            backup_buffer: Vec::new(),
            open: false, // 初始状态为关闭（不可见）
        });

        id
    }

    /// 任务2：打开并渲染已存在的窗口
    pub fn open_window(&mut self, display: &mut NotoFontDisplay, target_id: usize) {
        // 找到对应的窗口对象
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == target_id) {
            if win.open {
                return; // 如果已经打开，则不重复操作
            }

            // 1. 备份背景像素
            win.backup_buffer.clear(); // 确保 buffer 干净
            win.backup_buffer.reserve(win.width * win.height);
            for cy in win.y..win.y + win.height {
                for cx in win.x..win.x + win.width {
                    let index = cy * display.stride + cx;
                    if index < display.draw_buffer.len() {
                        win.backup_buffer.push(display.draw_buffer[index]);
                    } else {
                        win.backup_buffer.push(0);
                    }
                }
            }

            // 2. 绘制窗口实体
            let bg_color = 0x000000; // 黑色背景
            let border_color = 0x333333; // 深灰边框
            let title_bar_color = 0x4477AA; // 蓝色标题栏

            display.fill_rect(win.x, win.y, win.width, win.height, bg_color);
            display.fill_rect(win.x, win.y, win.width, 24, title_bar_color); // 标题栏
            display.draw_rect(win.x, win.y, win.width, win.height, border_color); // 边框

            // 绘制标题
            display.draw_string_at(&win.title, win.x + 5, win.y + 2, 0xFFFFFF);

            // 3. 标记为打开状态
            win.open = true;
        }
    }

    /// 任务3：关闭窗口，恢复背景（不从数组中移除）
    pub fn close_window(&mut self, display: &mut NotoFontDisplay, target_id: usize) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == target_id) {
            if !win.open {
                return; // 如果本来就是关闭的，不做操作
            }

            // 1. 恢复背景像素
            let mut i = 0;
            for cy in win.y..win.y + win.height {
                for cx in win.x..win.x + win.width {
                    let buffer_idx = cy * display.stride + cx;
                    if buffer_idx < display.draw_buffer.len() && i < win.backup_buffer.len() {
                        display.draw_buffer[buffer_idx] = win.backup_buffer[i];
                    }
                    i += 1;
                }
            }

            // 2. 清空备份以释放内存（可选，因为重新 open 会覆写）
            win.backup_buffer.clear();

            // 3. 仅修改状态，不移除
            win.open = false;
        }
    }
}

// 全局静态管理器实例
lazy_static! {
    pub static ref WINDOW_MANAGER: Mutex<WindowManager> = Mutex::new(WindowManager::new());
}

/// 任务5 & 任务6：绘制基础 GUI 桌面
pub fn draw_desktop(display: &mut NotoFontDisplay) {
    let screen_width = display.width;
    let screen_height = display.height;
    let screen_stride = display.stride;

    let logo = png_decoder::decode(crate::resources::LOGO).unwrap();
    let img_width = logo.0.width;
    let img_height = logo.0.height;
    let pixels = logo.1;
    
    let framebuffer = display.draw_buffer.as_mut_ptr();
    for cy in 0..img_height {
        for cx in 0..img_width {
            let src_index = cy * img_width + cx;
            let pixel = pixels[src_index as usize];

            // 只有当像素在屏幕范围内时才绘制
            let screen_x = cx; // 你可以加上偏移量来居中壁纸
            let screen_y = cy;

            if (screen_x as usize) < screen_width && (screen_y as usize) < screen_height {
                let a = pixel[3];
                if a > 0 {
                    let r = pixel[0];
                    let g = pixel[1];
                    let b = pixel[2];
                    let color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

                    // 统一使用 screen_stride 计算
                    let dst_index = screen_y as usize * screen_stride + screen_x as usize;
                    unsafe {
                        *framebuffer.add(dst_index) = color;
                    }
                }
            }
        }
    }

    // ---- 任务5：绘制左侧图标与文字 ----
    let icon_x = 120;

    // 假设你的解码器解出来的结构包含 width, height 和 像素切片(data)
    // 伪代码，请根据你实际的 png_decoder API 调整：
    let icon1 = png_decoder::decode(crate::resources::ICON1).unwrap();
    let icon2 = png_decoder::decode(crate::resources::ICON2).unwrap();
    display.draw_image(icon_x, 180, icon1);
    display.draw_string_at("terminal", icon_x, 250, 0x000000);
    display.draw_image(icon_x, 360, icon2);
    display.draw_string_at("settings", icon_x, 420, 0x000000);

    // *这里用占位色块模拟图标，防止因你尚未提供解码器API细节导致编译报错*
    // display.fill_rect(icon_x, 40, 48, 48, 0xFF5555); // 红色模拟 icon1
    let icon1_cp = png_decoder::decode(crate::resources::ICON1).unwrap();
    let icon2_cp = png_decoder::decode(crate::resources::ICON2).unwrap();
    let icon1_str = (icon_x, (icon1_cp.0.height as usize) + 180);
    let icon2_str = (icon_x, (icon2_cp.0.height as usize) + 360);

    //display.fill_rect(icon_x, 130, 48, 48, 0x5555FF); // 蓝色模拟 icon2

    // ---- 任务6：绘制底部任务栏 ----
    let taskbar_height = 30;
    let taskbar_y = display.height.saturating_sub(taskbar_height);

    // 绘制任务栏背景 (深灰色)
    display.fill_rect(0, taskbar_y, display.width, taskbar_height, 0x222222);
    // 任务栏上边框 (稍微亮一点的灰)
    display.draw_line(
        0,
        taskbar_y as isize,
        display.width as isize,
        taskbar_y as isize,
        0x555555,
    );

    // 绘制 Start 按钮区域与文字
    display.fill_rect(5, taskbar_y + 3, 60, taskbar_height - 6, 0x444444);
    display.draw_string_at("start", 15, taskbar_y + 5, 0xFFFFFF);
}
