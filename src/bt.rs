//! Bluetooth through BlueZ's bluetoothctl.

use crate::util::{run, run_timeout, short_err};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct BtState {
    pub service: bool,
    pub adapter: bool,
    pub powered: bool,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Device {
    pub mac: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

fn ctl(args: &[&str], secs: u64) -> (bool, String, String) {
    run_timeout("bluetoothctl", args, Duration::from_secs(secs))
}

pub fn state() -> BtState {
    let mut st = BtState::default();
    let (ok, _, _) = run("systemctl", &["is-active", "--quiet", "bluetooth"]);
    st.service = ok;
    if !ok {
        return st;
    }
    let (_, out, _) = ctl(&["show"], 5);
    if out.contains("No default controller") || !out.contains("Controller") {
        return st;
    }
    st.adapter = true;
    for l in out.lines() {
        let l = l.trim();
        if let Some(v) = l.strip_prefix("Powered:") {
            st.powered = v.trim() == "yes";
        } else if let Some(v) = l.strip_prefix("Alias:") {
            st.name = v.trim().to_string();
        }
    }
    st
}

pub fn start_service() -> Result<(), String> {
    let (ok, _, err) = run("systemctl", &["start", "bluetooth"]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn set_power(on: bool) -> Result<(), String> {
    if on {
        start_service()?;
        let _ = run("rfkill", &["unblock", "bluetooth"]);
    }
    let (ok, out, err) = ctl(&["power", if on { "on" } else { "off" }], 10);
    if ok && !out.contains("Failed") { Ok(()) } else { Err(short_err(if err.is_empty() { &out } else { &err })) }
}

fn list(filter: Option<&str>) -> Vec<(String, String)> {
    let mut args = vec!["devices"];
    if let Some(f) = filter {
        args.push(f);
    }
    let (_, out, _) = ctl(&args, 5);
    out.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            Some((mac.to_string(), name.trim().to_string()))
        })
        .collect()
}

pub fn devices() -> Vec<Device> {
    let paired: Vec<String> = list(Some("Paired")).into_iter().map(|d| d.0).collect();
    let connected: Vec<String> = list(Some("Connected")).into_iter().map(|d| d.0).collect();
    let mut v: Vec<Device> = list(None)
        .into_iter()
        .map(|(mac, name)| Device {
            paired: paired.contains(&mac),
            connected: connected.contains(&mac),
            name: if name.is_empty() || name.replace('-', ":") == mac { mac.clone() } else { name },
            mac,
        })
        .collect();
    v.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(a.name.cmp(&b.name)));
    v
}

pub fn scan(secs: u64) -> Result<Vec<Device>, String> {
    let s = secs.to_string();
    let _ = ctl(&["--timeout", &s, "scan", "on"], secs + 5);
    Ok(devices())
}

fn check(r: (bool, String, String)) -> Result<(), String> {
    let (ok, out, err) = r;
    let bad = out.contains("Failed") || out.contains("not available") || out.contains("Error");
    if ok && !bad {
        Ok(())
    } else {
        let msg = out.lines().rev().find(|l| l.contains("Failed") || l.contains("Error")).unwrap_or(err.as_str()).to_string();
        Err(short_err(&msg))
    }
}

pub fn pair(mac: &str) -> Result<(), String> {
    check(ctl(&["--agent", "NoInputNoOutput", "pair", mac], 40))?;
    let _ = ctl(&["trust", mac], 5);
    check(ctl(&["connect", mac], 20))
}

pub fn connect(mac: &str) -> Result<(), String> {
    check(ctl(&["connect", mac], 20))
}

pub fn disconnect(mac: &str) -> Result<(), String> {
    check(ctl(&["disconnect", mac], 10))
}

pub fn remove(mac: &str) -> Result<(), String> {
    check(ctl(&["remove", mac], 10))
}
