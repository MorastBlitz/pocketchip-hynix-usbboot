//! System info, battery (AXP209) and backlight via /proc and /sys.

use crate::util::{read_i64, read_trim, run_timeout, write_str};
use std::time::Duration;

const BAT: &str = "/sys/class/power_supply/axp20x-battery";
const USB: &str = "/sys/class/power_supply/axp20x-usb";
const AC: &str = "/sys/class/power_supply/axp20x-ac";

#[derive(Default, Clone)]
pub struct Battery {
    pub present: bool,
    pub capacity: i64,
    pub status: String,
    pub voltage_uv: i64,
    pub current_ua: i64,
    pub usb_online: bool,
    pub usb_voltage_uv: i64,
    pub usb_current_ua: i64,
    pub ac_online: bool,
    pub health: String,
}

impl Battery {
    pub fn read() -> Battery {
        let present = read_i64(&format!("{BAT}/present")).unwrap_or(0) == 1;
        Battery {
            present,
            capacity: read_i64(&format!("{BAT}/capacity")).unwrap_or(-1),
            status: read_trim(&format!("{BAT}/status")).unwrap_or_default(),
            voltage_uv: read_i64(&format!("{BAT}/voltage_now")).unwrap_or(0),
            current_ua: read_i64(&format!("{BAT}/current_now")).unwrap_or(0),
            usb_online: read_i64(&format!("{USB}/online")).unwrap_or(0) == 1,
            usb_voltage_uv: read_i64(&format!("{USB}/voltage_now")).unwrap_or(0),
            usb_current_ua: read_i64(&format!("{USB}/current_now")).unwrap_or(0),
            ac_online: read_i64(&format!("{AC}/online")).unwrap_or(0) == 1,
            health: read_trim(&format!("{BAT}/health")).unwrap_or_default(),
        }
    }

    pub fn charging(&self) -> bool {
        self.status == "Charging"
    }

    pub fn external_power(&self) -> bool {
        self.usb_online || self.ac_online
    }

    pub fn status_de(&self) -> &'static str {
        if !self.present {
            return "KEIN AKKU";
        }
        match self.status.as_str() {
            "Charging" => "LÄDT",
            "Discharging" => "ENTLÄDT",
            "Full" => "VOLL",
            "Not charging" => "LÄDT NICHT",
            _ => "UNBEKANNT",
        }
    }

    pub fn short(&self) -> String {
        if !self.present {
            return "NETZ".into();
        }
        format!("{}%{}", self.capacity, if self.charging() { "↑" } else { "" })
    }
}

fn hwmon_temp(name: &str) -> Option<f64> {
    let dir = std::fs::read_dir("/sys/class/hwmon").ok()?;
    for e in dir.flatten() {
        let p = e.path();
        if read_trim(&format!("{}/name", p.display())).as_deref() == Some(name) {
            return read_i64(&format!("{}/temp1_input", p.display())).map(|v| v as f64 / 1000.0);
        }
    }
    None
}

#[derive(Default)]
pub struct CpuSampler {
    last_total: u64,
    last_idle: u64,
    pub load: f64,
}

impl CpuSampler {
    pub fn sample(&mut self) {
        let Some(s) = read_trim("/proc/stat") else { return };
        let Some(line) = s.lines().next() else { return };
        let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        if v.len() < 5 {
            return;
        }
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        let total: u64 = v.iter().sum();
        let dt = total.saturating_sub(self.last_total);
        let di = idle.saturating_sub(self.last_idle);
        if self.last_total > 0 && dt > 0 {
            self.load = 100.0 * (1.0 - di as f64 / dt as f64);
        }
        self.last_total = total;
        self.last_idle = idle;
    }
}

pub struct SysInfo {
    pub model: String,
    pub hostname: String,
    pub kernel: String,
    pub cpu_model: String,
    pub cpu_mhz: i64,
    pub loadavg: String,
    pub mem_total_kb: i64,
    pub mem_avail_kb: i64,
    pub swap_total_kb: i64,
    pub swap_free_kb: i64,
    pub uptime_s: u64,
    pub disk_total: u64,
    pub disk_free: u64,
    pub soc_temp: Option<f64>,
    pub pmic_temp: Option<f64>,
}

fn meminfo(key: &str, text: &str) -> i64 {
    text.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

impl SysInfo {
    pub fn read() -> SysInfo {
        let mem = read_trim("/proc/meminfo").unwrap_or_default();
        let cpu = read_trim("/proc/cpuinfo").unwrap_or_default();
        let model = cpu
            .lines()
            .find(|l| l.starts_with("model name") || l.starts_with("Processor"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_else(|| "ARM".into());
        let hw = cpu
            .lines()
            .find(|l| l.starts_with("Hardware"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string());
        let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
        unsafe { libc::uname(&mut uts) };
        let cstr = |a: &[libc::c_char]| unsafe { std::ffi::CStr::from_ptr(a.as_ptr()) }.to_string_lossy().into_owned();
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        let (dt, df) = if unsafe { libc::statvfs(c"/".as_ptr(), &mut st) } == 0 {
            (st.f_blocks as u64 * st.f_frsize as u64, st.f_bavail as u64 * st.f_frsize as u64)
        } else {
            (0, 0)
        };
        let part = cpu
            .lines()
            .find(|l| l.starts_with("CPU part"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default();
        let core = match part.as_str() {
            "0xc08" => Some("CORTEX-A8"),
            "0xc07" => Some("CORTEX-A7"),
            "0xc09" => Some("CORTEX-A9"),
            _ => None,
        };
        let model = core.map(|c| format!("{c} (ARMV7)")).unwrap_or(model);
        let dt_model = std::fs::read("/proc/device-tree/model")
            .ok()
            .map(|b| String::from_utf8_lossy(&b).trim_end_matches('\0').trim().to_string())
            .unwrap_or_default();
        SysInfo {
            model: dt_model.to_uppercase(),
            hostname: cstr(&uts.nodename),
            kernel: cstr(&uts.release),
            cpu_model: if core.is_some() { model } else { hw.map(|h| format!("{model} / {h}")).unwrap_or(model) },
            cpu_mhz: read_i64("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").unwrap_or(0) / 1000,
            loadavg: read_trim("/proc/loadavg")
                .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
                .unwrap_or_default(),
            mem_total_kb: meminfo("MemTotal:", &mem),
            mem_avail_kb: meminfo("MemAvailable:", &mem),
            swap_total_kb: meminfo("SwapTotal:", &mem),
            swap_free_kb: meminfo("SwapFree:", &mem),
            uptime_s: read_trim("/proc/uptime")
                .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
                .unwrap_or(0.0) as u64,
            disk_total: dt,
            disk_free: df,
            soc_temp: hwmon_temp("sun4i_ts"),
            pmic_temp: hwmon_temp("pmic_temp"),
        }
    }
}

/// IPv4 addresses of all global interfaces, e.g. "wlan0 10.0.0.2".
pub fn ip_addrs() -> Vec<(String, String)> {
    let (_, out, _) = run_timeout("ip", &["-4", "-o", "addr", "show", "scope", "global"], Duration::from_secs(3));
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let i = f.iter().position(|&x| x == "inet")?;
            Some((f.get(1)?.to_string(), f.get(i + 1)?.split('/').next()?.to_string()))
        })
        .collect()
}

pub fn fmt_bytes(b: u64) -> String {
    let mb = b as f64 / 1048576.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

// ---- backlight ----------------------------------------------------------

pub struct Backlight {
    dir: Option<String>,
    pub max: i64,
}

impl Backlight {
    pub fn open() -> Backlight {
        let dir = std::fs::read_dir("/sys/class/backlight")
            .ok()
            .and_then(|mut d| d.next())
            .and_then(|e| e.ok())
            .map(|e| e.path().display().to_string());
        let max = dir.as_ref().and_then(|d| read_i64(&format!("{d}/max_brightness"))).unwrap_or(10);
        Backlight { dir, max }
    }

    pub fn set(&self, v: i64) {
        if let Some(d) = &self.dir {
            write_str(&format!("{d}/brightness"), &v.clamp(1, self.max).to_string());
        }
    }

    pub fn power(&self, on: bool) {
        if let Some(d) = &self.dir {
            write_str(&format!("{d}/bl_power"), if on { "0" } else { "4" });
        }
    }
}
