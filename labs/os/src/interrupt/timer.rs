use crate::sbi::set_timer;
use riscv::register::{sie, sstatus, time};

/// 开启时钟中断使能，并且预约第一次时钟中断
pub fn init() {
    unsafe {
        // 开启 STIE，允许时钟中断
        sie::set_stimer();
    }
    set_next_timeout();
}

/// 触发时钟中断计数
pub static mut TICKS: usize = 0;

/// 每一次时钟中断时调用
///
/// 设置下一次时钟中断，同时计数 +1
pub fn tick() {
    set_next_timeout();
    unsafe {
        TICKS += 1;
        if TICKS % 100 == 0 {
            println!("{} tick", TICKS);
        }
    }
}

/// 时钟中断的间隔，单位是 CPU 指令
static INTERVAL: usize = 100000;

/// 设置下一次时钟中断
///
/// 获取当前时间，加上中断间隔，通过 SBI 调用预约下一次中断
fn set_next_timeout() {
    set_timer(time::read() + INTERVAL);
}
