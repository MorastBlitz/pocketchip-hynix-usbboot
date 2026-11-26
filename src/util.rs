use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run a command, wait at most `timeout`, return (success, stdout, stderr).
pub fn run_timeout(prog: &str, args: &[&str], timeout: Duration) -> (bool, String, String) {
    let child = Command::new(prog)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return (false, String::new(), format!("{prog}: {e}")),
    };
    // Drain pipes in helper threads so a chatty child cannot block.
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break None,
        }
    };
    let out = to.join().unwrap_or_default();
    let mut err = te.join().unwrap_or_default();
    if status.is_none() {
        err = format!("Zeitüberschreitung ({prog})");
    }
    (status.map(|s| s.success()).unwrap_or(false), out, err)
}

pub fn run(prog: &str, args: &[&str]) -> (bool, String, String) {
    run_timeout(prog, args, Duration::from_secs(20))
}

pub fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn read_i64(path: &str) -> Option<i64> {
    read_trim(path)?.parse().ok()
}

pub fn write_str(path: &str, v: &str) -> bool {
    std::fs::write(path, v).is_ok()
}

/// First line of an error text, upper-cased and shortened for the footer.
pub fn short_err(s: &str) -> String {
    let line = s
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("UNBEKANNTER FEHLER");
    let line = line.trim_start_matches("Error: ");
    let mut t: String = line.chars().take(60).collect();
    t = t.to_uppercase();
    t
}

pub fn clock_ms(clock: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(clock, &mut ts) };
    ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000
}

pub fn mono_ms() -> u64 {
    clock_ms(libc::CLOCK_MONOTONIC)
}

/// Time spent in suspend so far (BOOTTIME keeps counting, MONOTONIC does not).
pub fn suspended_ms() -> u64 {
    #[cfg(target_os = "linux")]
    {
        clock_ms(libc::CLOCK_BOOTTIME).saturating_sub(clock_ms(libc::CLOCK_MONOTONIC))
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

pub fn bar(pct: i64, width: usize) -> String {
    let pct = pct.clamp(0, 100) as usize;
    let full = (pct * width + 50) / 100;
    let mut s = String::new();
    for i in 0..width {
        s.push(if i < full { '█' } else { '░' });
    }
    s
}

pub fn fmt_duration(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}T {h:02}:{m:02}")
    } else {
        format!("{h:02}:{m:02}:{:02}", secs % 60)
    }
}
