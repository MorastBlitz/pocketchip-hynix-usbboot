//! WLAN through NetworkManager's nmcli (terse output).

use crate::util::{run, run_timeout, short_err};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct Network {
    pub ssid: String,
    pub signal: i64,
    pub security: String,
    pub in_use: bool,
    pub known: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WifiState {
    pub radio: bool,
    pub device: Option<String>,
    pub connection: Option<String>,
    pub ssid: Option<String>,
}

/// Split one line of `nmcli -t` output on unescaped ':'.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in line.chars() {
        if esc {
            out.last_mut().unwrap().push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == ':' {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
    }
    out
}

pub fn state() -> WifiState {
    let mut st = WifiState::default();
    let (_, out, _) = run_timeout("nmcli", &["-t", "-f", "WIFI", "general"], Duration::from_secs(5));
    st.radio = out.trim() == "enabled";
    let (_, out, _) = run_timeout("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"], Duration::from_secs(5));
    for l in out.lines() {
        let f = split_terse(l);
        if f.len() >= 4 && f[1] == "wifi" {
            st.device = Some(f[0].clone());
            if f[2] == "connected" && !f[3].is_empty() {
                st.connection = Some(f[3].clone());
            }
            break;
        }
    }
    if let Some(c) = &st.connection {
        let (_, out, _) = run_timeout("nmcli", &["-g", "802-11-wireless.ssid", "connection", "show", c], Duration::from_secs(5));
        let s = out.trim().replace("\\:", ":");
        st.ssid = Some(if s.is_empty() { c.clone() } else { s });
    }
    st
}

/// Saved wifi connections: (connection name, ssid).
pub fn known() -> Vec<(String, String)> {
    let (_, out, _) = run_timeout("nmcli", &["-t", "-f", "NAME,TYPE", "connection", "show"], Duration::from_secs(5));
    let mut v = Vec::new();
    for l in out.lines() {
        let f = split_terse(l);
        if f.len() >= 2 && f[1] == "802-11-wireless" {
            let (_, s, _) = run_timeout("nmcli", &["-g", "802-11-wireless.ssid", "connection", "show", &f[0]], Duration::from_secs(5));
            let ssid = s.trim().replace("\\:", ":");
            v.push((f[0].clone(), if ssid.is_empty() { f[0].clone() } else { ssid }));
        }
    }
    v
}

pub fn scan(rescan: bool) -> Result<Vec<Network>, String> {
    let (ok, out, err) = run_timeout(
        "nmcli",
        &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", if rescan { "yes" } else { "no" }],
        Duration::from_secs(30),
    );
    if !ok {
        return Err(short_err(&err));
    }
    let known: Vec<String> = known().into_iter().map(|(_, s)| s).collect();
    let mut nets: Vec<Network> = Vec::new();
    for l in out.lines() {
        let f = split_terse(l);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let n = Network {
            in_use: f[0] == "*",
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0),
            security: f[3].clone(),
            known: known.contains(&f[1]),
        };
        if let Some(e) = nets.iter_mut().find(|e| e.ssid == n.ssid) {
            e.in_use |= n.in_use;
            if n.signal > e.signal {
                e.signal = n.signal;
            }
        } else {
            nets.push(n);
        }
    }
    nets.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    Ok(nets)
}

pub fn set_radio(on: bool) -> Result<(), String> {
    let (ok, _, err) = run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let before = known();
    // A saved profile is brought up directly; with a new password it is
    // updated first. Saved profiles are never deleted here.
    if let Some((name, _)) = before.iter().find(|(_, s)| s == ssid) {
        if let Some(p) = password {
            let (ok, _, err) = run("nmcli", &["connection", "modify", "id", name, "wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.psk", p]);
            if !ok {
                return Err(short_err(&err));
            }
        }
        let (ok, _, err) = run_timeout("nmcli", &["--wait", "40", "connection", "up", "id", name], Duration::from_secs(45));
        return if ok { Ok(()) } else { Err(short_err(&err)) };
    }
    let mut args = vec!["--wait", "40", "device", "wifi", "connect", ssid];
    if let Some(p) = password {
        args.push("password");
        args.push(p);
    }
    let (ok, _, err) = run_timeout("nmcli", &args, Duration::from_secs(45));
    if ok {
        Ok(())
    } else {
        // Remove only the profile this attempt created (e.g. wrong password).
        for (name, s) in known() {
            if s == ssid && !before.iter().any(|(n, _)| *n == name) {
                let _ = run("nmcli", &["connection", "delete", "id", &name]);
            }
        }
        Err(short_err(&err))
    }
}

pub fn disconnect(device: &str) -> Result<(), String> {
    let (ok, _, err) = run("nmcli", &["device", "disconnect", device]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn forget(ssid: &str) -> Result<(), String> {
    let mut any = false;
    for (name, s) in known() {
        if s == ssid {
            let (ok, _, err) = run("nmcli", &["connection", "delete", "id", &name]);
            if !ok {
                return Err(short_err(&err));
            }
            any = true;
        }
    }
    if any { Ok(()) } else { Err("KEIN GESPEICHERTES PROFIL".into()) }
}

pub fn signal_bars(sig: i64) -> &'static str {
    match sig {
        s if s >= 75 => "▮▮▮▮",
        s if s >= 50 => "▮▮▮▯",
        s if s >= 25 => "▮▮▯▯",
        _ => "▮▯▯▯",
    }
}
