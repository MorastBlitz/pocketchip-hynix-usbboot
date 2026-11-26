use std::collections::BTreeMap;

const PATH: &str = "/var/lib/pocketchip-console/settings.conf";

pub const SCREEN_STEPS: &[u32] = &[0, 15, 30, 60, 120, 300, 600, 1800];
pub const STANDBY_STEPS: &[u32] = &[0, 60, 300, 600, 900, 1800, 3600];

#[derive(Clone, Debug)]
pub struct Settings {
    pub brightness: i64,
    pub screen_timeout: u32,
    pub standby_timeout: u32,
    pub standby_on_battery_only: bool,
    pub scanlines: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            brightness: 8,
            screen_timeout: 60,
            standby_timeout: 600,
            standby_on_battery_only: true,
            scanlines: true,
        }
    }
}

fn path() -> String {
    std::env::var("POCKETCHIP_CONSOLE_SETTINGS").unwrap_or_else(|_| PATH.to_string())
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "brightness" => s.brightness = v.parse().unwrap_or(s.brightness),
                "screen_timeout" => s.screen_timeout = v.parse().unwrap_or(s.screen_timeout),
                "standby_timeout" => s.standby_timeout = v.parse().unwrap_or(s.standby_timeout),
                "standby_on_battery_only" => s.standby_on_battery_only = v == "1",
                "scanlines" => s.scanlines = v == "1",
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let mut m = BTreeMap::new();
        m.insert("brightness", self.brightness.to_string());
        m.insert("screen_timeout", self.screen_timeout.to_string());
        m.insert("standby_timeout", self.standby_timeout.to_string());
        m.insert("standby_on_battery_only", (self.standby_on_battery_only as u8).to_string());
        m.insert("scanlines", (self.scanlines as u8).to_string());
        let text: String = m.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let p = path();
        if let Some(dir) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = format!("{p}.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

pub fn step(steps: &[u32], cur: u32, delta: i32) -> u32 {
    let idx = steps.iter().position(|&s| s >= cur).unwrap_or(steps.len() - 1) as i32;
    let n = (idx + delta).clamp(0, steps.len() as i32 - 1);
    steps[n as usize]
}

pub fn fmt_timeout(secs: u32) -> String {
    match secs {
        0 => "NIE".into(),
        s if s < 60 => format!("{s} S"),
        s if s < 3600 => format!("{} MIN", s / 60),
        s => format!("{} STD", s / 3600),
    }
}
