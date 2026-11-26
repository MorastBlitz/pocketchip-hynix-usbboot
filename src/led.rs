//! Status LEDs: the white CHIP status LED (Linux LED class) and the AXP209
//! charge LED (CHGLED, register 0x32 over i2c). Both shine through the same
//! light pipe and mix to purple.

use crate::util::write_str;
use std::os::fd::AsRawFd;

const WHITE: &str = "/sys/class/leds/chip:white:status";
const I2C_DEV: &str = "/dev/i2c-0";
const AXP_ADDR: libc::c_ulong = 0x34;
const REG_OFF_CTRL: u8 = 0x32;
const I2C_SLAVE_FORCE: libc::c_ulong = 0x0706;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// system default: white on, charge LED driven by the charger
    Normal,
    Off,
    /// short white flash every 2 s, charge LED off
    Blink,
}

fn white(trigger: &str) {
    if trigger == "timer" {
        let _ = crate::util::run("modprobe", &["ledtrig-timer"]);
    }
    write_str(&format!("{WHITE}/trigger"), trigger);
    match trigger {
        "none" => {
            write_str(&format!("{WHITE}/brightness"), "0");
        }
        "timer" => {
            write_str(&format!("{WHITE}/delay_on"), "80");
            write_str(&format!("{WHITE}/delay_off"), "1920");
        }
        _ => {}
    }
}

/// Read-modify-write of the CHGLED bits (5:3) only. Bit 7 of this register
/// powers the PMIC off, so it is always written back as 0.
fn chgled(by_register_off: bool) -> bool {
    let _ = crate::util::run("modprobe", &["i2c-dev"]);
    let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open(I2C_DEV) else { return false };
    let fd = f.as_raw_fd();
    unsafe {
        if libc::ioctl(fd, I2C_SLAVE_FORCE as _, AXP_ADDR) != 0 {
            return false;
        }
        let reg = [REG_OFF_CTRL];
        if libc::write(fd, reg.as_ptr() as *const libc::c_void, 1) != 1 {
            return false;
        }
        let mut v = [0u8];
        if libc::read(fd, v.as_mut_ptr() as *mut libc::c_void, 1) != 1 {
            return false;
        }
        let mut n = v[0] & 0x47; // keep bit 6 and bits 2:0, clear 7 and 5:3
        if by_register_off {
            n |= 0x08; // bit 3: CHGLED controlled by bits 5:4 = 00 (off)
        }
        if n == v[0] {
            return true;
        }
        let w = [REG_OFF_CTRL, n];
        libc::write(fd, w.as_ptr() as *const libc::c_void, 2) == 2
    }
}

pub fn apply(mode: Mode) {
    match mode {
        Mode::Normal => {
            white("default-on");
            chgled(false);
        }
        Mode::Off => {
            white("none");
            chgled(true);
        }
        Mode::Blink => {
            white("timer");
            chgled(true);
        }
    }
}
