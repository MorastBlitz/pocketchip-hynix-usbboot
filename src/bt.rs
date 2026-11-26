//! Bluetooth through BlueZ's bluetoothctl.

use crate::util::{run, run_timeout, short_err};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct BtState {
    pub service: bool,
    pub adapter: bool,
    pub powered: bool,
    /// soft-blocked via rfkill (switched off in the console, survives reboots)
    pub blocked: bool,
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
    st.blocked = rfkill_blocked();
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

/// sysfs dirs of all Bluetooth rfkill switches (looked up by type: the
/// index changes when the driver is re-probed).
fn rfkill_dirs() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/sys/class/rfkill") else { return Vec::new() };
    dir.flatten()
        .map(|e| e.path().display().to_string())
        .filter(|p| crate::util::read_trim(&format!("{p}/type")).as_deref() == Some("bluetooth"))
        .collect()
}

pub fn rfkill_blocked() -> bool {
    rfkill_dirs().iter().any(|p| crate::util::read_trim(&format!("{p}/soft")).as_deref() == Some("1"))
}

/// Soft-block or unblock Bluetooth. systemd-rfkill restores this state at
/// boot (BlueZ AutoEnable would otherwise power the controller on again);
/// its state file is written right away too, so a hard power loss keeps it.
fn rfkill_set(block: bool) {
    let v = if block { "1" } else { "0" };
    for p in rfkill_dirs() {
        crate::util::write_str(&format!("{p}/soft"), v);
    }
    if let Ok(dir) = std::fs::read_dir("/var/lib/systemd/rfkill") {
        for e in dir.flatten() {
            if e.file_name().to_string_lossy().ends_with(":bluetooth") {
                let _ = std::fs::write(e.path(), format!("{v}\n"));
            }
        }
    }
}

pub fn set_power(on: bool) -> Result<(), String> {
    if on {
        rfkill_set(false);
        start_service()?;
        // the controller needs a moment after the unblock
        let mut last = String::new();
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let (ok, out, err) = ctl(&["power", "on"], 10);
            if ok && !out.contains("Failed") {
                return Ok(());
            }
            last = if err.is_empty() { out } else { err };
        }
        Err(short_err(&last))
    } else {
        let _ = ctl(&["power", "off"], 10);
        rfkill_set(true);
        Ok(())
    }
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
    // connected, paired, then devices with a real name; bare addresses last
    let named = |d: &Device| d.name != d.mac;
    v.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(named(b).cmp(&named(a)))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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

fn info_flag(mac: &str, key: &str) -> bool {
    let (_, out, _) = ctl(&["info", mac], 5);
    out.lines().any(|l| l.trim() == format!("{key}: yes"))
}

/// Pair inside one interactive bluetoothctl session with a NoInputNoOutput
/// agent ("just works"; a phone asks for confirmation on its side), wait for
/// the result, then trust and connect. Success is checked with `info`.
pub fn pair(mac: &str) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let mut child = Command::new("bluetoothctl")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| short_err(&e.to_string()))?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let send = |stdin: &mut std::process::ChildStdin, c: &str| {
        let _ = writeln!(stdin, "{c}");
        let _ = stdin.flush();
    };
    send(&mut stdin, "agent NoInputNoOutput");
    send(&mut stdin, "default-agent");
    std::thread::sleep(Duration::from_millis(500));
    send(&mut stdin, &format!("pair {mac}"));
    let start = Instant::now();
    let mut result: Option<Result<(), String>> = None;
    while start.elapsed() < Duration::from_secs(45) && result.is_none() {
        let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) else { continue };
        let l = line.to_lowercase();
        if l.contains("pairing successful") || l.contains("alreadyexists") {
            result = Some(Ok(()));
        } else if l.contains("failed to pair") || l.contains("authenticationfailed") || l.contains("authenticationcanceled") || l.contains("not available") {
            let msg = line.split("org.bluez.Error.").nth(1).unwrap_or(&line).to_string();
            result = Some(Err(short_err(&format!("KOPPELN FEHLGESCHLAGEN: {msg}"))));
        }
    }
    if result.as_ref().map(|r| r.is_ok()).unwrap_or(false) {
        send(&mut stdin, &format!("trust {mac}"));
        std::thread::sleep(Duration::from_millis(500));
    }
    send(&mut stdin, "quit");
    let _ = child.wait();
    match result {
        Some(Err(e)) => return Err(e),
        None => return Err("KOPPELN: ZEITÜBERSCHREITUNG".into()),
        Some(Ok(())) => {}
    }
    if !info_flag(mac, "Paired") {
        return Err("KOPPELN NICHT BESTÄTIGT".into());
    }
    // pairing alone is useful (e.g. a phone); connecting needs a profile
    match connect(mac) {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("GEKOPPELT, ABER NICHT VERBUNDEN: {e}")),
    }
}

pub fn connect(mac: &str) -> Result<(), String> {
    check(ctl(&["connect", mac], 20))?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    if info_flag(mac, "Connected") { Ok(()) } else { Err("VERBINDUNG ABGEBROCHEN".into()) }
}

pub fn disconnect(mac: &str) -> Result<(), String> {
    check(ctl(&["disconnect", mac], 10))
}

pub fn remove(mac: &str) -> Result<(), String> {
    check(ctl(&["remove", mac], 10))
}

#[derive(Clone, Debug, Default)]
pub struct DevInfo {
    pub mac: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub bonded: bool,
    pub trusted: bool,
    pub connected: bool,
    pub blocked: bool,
    pub rssi: Option<i32>,
    pub battery: Option<i32>,
    pub services: Vec<String>,
    pub known: bool,
}

fn paren_num(v: &str) -> Option<i32> {
    v.split('(').nth(1)?.trim_end_matches(')').trim().parse().ok()
}

pub fn info(mac: &str) -> DevInfo {
    let (_, out, _) = ctl(&["info", mac], 5);
    let mut d = DevInfo { mac: mac.to_string(), ..Default::default() };
    for l in out.lines() {
        let l = l.trim();
        if l.starts_with("Device ") {
            d.known = true;
        }
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k {
            "Name" => d.name = v.to_string(),
            "Alias" if d.name.is_empty() => d.name = v.to_string(),
            "Icon" => d.icon = v.to_string(),
            "Paired" => d.paired = v == "yes",
            "Bonded" => d.bonded = v == "yes",
            "Trusted" => d.trusted = v == "yes",
            "Connected" => d.connected = v == "yes",
            "Blocked" => d.blocked = v == "yes",
            "RSSI" => d.rssi = paren_num(v).or_else(|| v.parse().ok()),
            "Battery Percentage" => d.battery = paren_num(v),
            "UUID" => {
                let name = v.split('(').next().unwrap_or(v).trim();
                if !name.is_empty() && !name.starts_with("Vendor specific") && !d.services.iter().any(|s| s == name) {
                    d.services.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    if d.name.is_empty() {
        d.name = mac.to_string();
    }
    d
}

pub fn icon_de(icon: &str) -> &'static str {
    match icon {
        "phone" => "TELEFON",
        "computer" => "COMPUTER",
        "audio-card" | "audio-headphones" | "audio-headset" => "AUDIO",
        "input-keyboard" => "TASTATUR",
        "input-mouse" => "MAUS",
        "input-gaming" => "GAMEPAD",
        "input-tablet" => "TABLET",
        "printer" => "DRUCKER",
        "camera-photo" | "camera-video" => "KAMERA",
        "modem" | "network-wireless" => "NETZWERK",
        "" => "UNBEKANNT",
        _ => "SONSTIGES",
    }
}
