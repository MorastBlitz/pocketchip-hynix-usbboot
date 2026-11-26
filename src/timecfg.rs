//! Date, time and time zone through timedatectl.

use crate::util::{run, run_timeout, short_err};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct TimeState {
    pub timezone: String,
    pub ntp: bool,
    pub synced: bool,
}

pub fn state() -> TimeState {
    let (_, out, _) = run_timeout("timedatectl", &["show"], Duration::from_secs(5));
    let mut st = TimeState::default();
    for l in out.lines() {
        if let Some((k, v)) = l.split_once('=') {
            match k {
                "Timezone" => st.timezone = v.to_string(),
                "NTP" => st.ntp = v == "yes",
                "NTPSynchronized" => st.synced = v == "yes",
                _ => {}
            }
        }
    }
    st
}

pub fn zones() -> Vec<String> {
    let (_, out, _) = run_timeout("timedatectl", &["list-timezones"], Duration::from_secs(10));
    out.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn set_ntp(on: bool) -> Result<(), String> {
    let (ok, _, err) = run("timedatectl", &["set-ntp", if on { "true" } else { "false" }]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn set_zone(z: &str) -> Result<(), String> {
    let (ok, _, err) = run("timedatectl", &["set-timezone", z]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn set_time(t: &Local) -> Result<(), String> {
    let s = format!("{:04}-{:02}-{:02} {:02}:{:02}:00", t.year, t.month, t.day, t.hour, t.minute);
    let st = state();
    if st.ntp {
        set_ntp(false)?;
    }
    let (ok, _, err) = run("timedatectl", &["set-time", &s]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Local {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub wday: i32,
}

extern "C" {
    fn tzset();
}

/// glibc caches /etc/localtime; point TZ at the zone so a change shows up.
pub fn apply_tz(zone: &str) {
    if zone.is_empty() {
        return;
    }
    std::env::set_var("TZ", format!(":{zone}"));
    unsafe { tzset() };
}

pub fn now() -> Local {
    unsafe {
        // re-read TZ so a changed time zone shows up without restart
        tzset();
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        Local {
            year: tm.tm_year + 1900,
            month: tm.tm_mon + 1,
            day: tm.tm_mday,
            hour: tm.tm_hour,
            minute: tm.tm_min,
            second: tm.tm_sec,
            wday: tm.tm_wday,
        }
    }
}

pub fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }
        }
    }
}

pub const WEEKDAYS: [&str; 7] = ["SO", "MO", "DI", "MI", "DO", "FR", "SA"];
