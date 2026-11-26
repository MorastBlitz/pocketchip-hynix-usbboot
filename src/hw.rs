//! Raw evdev watcher (activity + KEY_POWER), VT mode and signal handling.

use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub static LAST_ACTIVITY: AtomicU64 = AtomicU64::new(0);
/// While set, finished touches are reported as raw normalized points.
pub static CALIBRATING: AtomicBool = AtomicBool::new(false);

pub enum InputEvent {
    Activity,
    PowerKey,
    /// Raw touch position (0..1 in device axes), averaged over one press.
    TouchRaw(f64, f64),
}

#[derive(Default, Clone, Copy)]
struct AbsRange {
    min: i32,
    max: i32,
}

#[derive(Default)]
struct TouchState {
    rx: AbsRange,
    ry: AbsRange,
    x: i32,
    y: i32,
    down: bool,
    sum_x: f64,
    sum_y: f64,
    n: u32,
    frames: u32,
}

fn abs_range(fd: i32, axis: u32) -> Option<AbsRange> {
    let mut info = [0i32; 6];
    let req: libc::c_ulong = 0x8018_4540 + axis as libc::c_ulong;
    if unsafe { libc::ioctl(fd, req as _, info.as_mut_ptr()) } == 0 && info[2] > info[1] {
        Some(AbsRange { min: info[1], max: info[2] })
    } else {
        None
    }
}

const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const KEY_POWER: u16 = 116;
const BTN_TOUCH: u16 = 330;
const EV_SYN: u16 = 0;

#[cfg(target_pointer_width = "32")]
const EV_SIZE: usize = 16;
#[cfg(target_pointer_width = "64")]
const EV_SIZE: usize = 24;

/// Watch all evdev devices without grabbing them; libinput keeps working.
pub fn spawn_input_watcher(cb: impl Fn(InputEvent) + Send + 'static) {
    std::thread::spawn(move || {
        let mut files = Vec::new();
        if let Ok(dir) = std::fs::read_dir("/dev/input") {
            for e in dir.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("event") {
                    if let Ok(f) = std::fs::File::open(e.path()) {
                        files.push(f);
                    }
                }
            }
        }
        if files.is_empty() {
            eprintln!("input watcher: no /dev/input/event* readable");
            return;
        }
        let mut touch: Vec<TouchState> = files
            .iter()
            .map(|f| {
                let fd = f.as_raw_fd();
                let mut t = TouchState::default();
                if let (Some(x), Some(y)) = (abs_range(fd, 0), abs_range(fd, 1)) {
                    t.rx = x;
                    t.ry = y;
                }
                t
            })
            .collect();
        let mut buf = [0u8; EV_SIZE * 32];
        let mut last_activity_sent = 0u64;
        loop {
            let mut fds: Vec<libc::pollfd> = files
                .iter()
                .map(|f| libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 })
                .collect();
            let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if n <= 0 {
                continue;
            }
            for (di, p) in fds.iter().enumerate() {
                if p.revents & libc::POLLIN == 0 {
                    continue;
                }
                let r = unsafe { libc::read(p.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if r <= 0 {
                    continue;
                }
                let mut activity = false;
                for ev in buf[..r as usize].chunks_exact(EV_SIZE) {
                    let off = EV_SIZE - 8;
                    let typ = u16::from_ne_bytes([ev[off], ev[off + 1]]);
                    let code = u16::from_ne_bytes([ev[off + 2], ev[off + 3]]);
                    let val = i32::from_ne_bytes([ev[off + 4], ev[off + 5], ev[off + 6], ev[off + 7]]);
                    if typ == EV_KEY && code == KEY_POWER {
                        if val == 1 {
                            cb(InputEvent::PowerKey);
                        }
                        continue;
                    }
                    if typ == EV_KEY || typ == EV_ABS {
                        activity = true;
                    }
                    let t = &mut touch[di];
                    if t.rx.max > 0 {
                        match (typ, code) {
                            (EV_ABS, 0) => t.x = val,
                            (EV_ABS, 1) => t.y = val,
                            (EV_KEY, BTN_TOUCH) => {
                                t.down = val != 0;
                                if !t.down && t.n == 0 && t.frames > 0 {
                                    t.sum_x = t.x as f64;
                                    t.sum_y = t.y as f64;
                                    t.n = 1;
                                }
                                if !t.down && t.n > 0 && CALIBRATING.load(Ordering::Relaxed) {
                                    let nx = (t.sum_x / t.n as f64 - t.rx.min as f64) / (t.rx.max - t.rx.min) as f64;
                                    let ny = (t.sum_y / t.n as f64 - t.ry.min as f64) / (t.ry.max - t.ry.min) as f64;
                                    cb(InputEvent::TouchRaw(nx, ny));
                                }
                                t.sum_x = 0.0;
                                t.sum_y = 0.0;
                                t.n = 0;
                                t.frames = 0;
                            }
                            (EV_SYN, 0) if t.down => {
                                // skip the first reports of a press, the stylus settles
                                t.frames += 1;
                                if t.frames > 2 {
                                    t.sum_x += t.x as f64;
                                    t.sum_y += t.y as f64;
                                    t.n += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if activity {
                    let now = crate::util::mono_ms();
                    LAST_ACTIVITY.store(now, Ordering::Relaxed);
                    // Limit wake-ups of the UI thread to a few per second.
                    if now.saturating_sub(last_activity_sent) > 250 {
                        last_activity_sent = now;
                        cb(InputEvent::Activity);
                    }
                }
            }
        }
    });
}

// ---- VT handling ---------------------------------------------------------

const KDSETMODE: libc::c_ulong = 0x4B3A;
const KD_TEXT: libc::c_ulong = 0;
const KD_GRAPHICS: libc::c_ulong = 1;
const KDGKBMODE: libc::c_ulong = 0x4B44;
const KDSKBMODE: libc::c_ulong = 0x4B45;
const K_OFF: libc::c_ulong = 4;

pub struct VtGuard {
    fd: i32,
    kbmode: libc::c_int,
}

impl VtGuard {
    /// If stdin is a VT, switch it to graphics mode and stop the kernel from
    /// feeding key presses into the console (libinput still sees them).
    pub fn take() -> Option<VtGuard> {
        let fd = 0;
        let mut kbmode: libc::c_int = 0;
        #[allow(clippy::useless_conversion)]
        let ok = unsafe { libc::ioctl(fd, KDGKBMODE as _, &mut kbmode) } == 0;
        if !ok {
            return None;
        }
        unsafe {
            libc::ioctl(fd, KDSETMODE as _, KD_GRAPHICS);
            libc::ioctl(fd, KDSKBMODE as _, K_OFF);
        }
        Some(VtGuard { fd, kbmode })
    }
}

impl Drop for VtGuard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.fd, KDSKBMODE as _, self.kbmode as libc::c_ulong);
            libc::ioctl(self.fd, KDSETMODE as _, KD_TEXT);
        }
    }
}

/// Block SIGTERM/SIGINT in all threads and turn them into a clean quit.
pub fn install_signal_quit() {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGHUP);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        let set_copy = set;
        std::thread::spawn(move || {
            let mut sig: libc::c_int = 0;
            libc::sigwait(&set_copy, &mut sig);
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        });
    }
}
