//! Bluetooth LE scanner for the RADAR page, straight on BlueZ's D-Bus API.
//!
//! Discovery runs with the filter Transport=le, DuplicateData=true, so every
//! advertisement updates the RSSI. BlueZ ties the discovery session to our
//! D-Bus connection: when the scanner stops (or the console dies) BlueZ ends
//! it. The adapter's power and rfkill state are never touched here.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::MatchRule;

use crate::util::mono_ms;

const DEVICE: &str = "org.bluez.Device1";
const ADAPTER: &str = "org.bluez.Adapter1";
const OWN_IFACE: &str = "de.pocketchip.Radar";
/// RSSI samples kept per device
const HISTORY: usize = 48;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Starting,
    Scanning,
    /// adapter present but powered off (also: rfkill blocked)
    BtOff,
    NoAdapter,
    /// bluetoothd not running
    NoService,
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct Dev {
    pub path: String,
    pub addr: String,
    /// "public" or "random" (BlueZ AddressType)
    pub addr_type: String,
    pub name: String,
    pub alias: String,
    pub icon: String,
    pub appearance: Option<u16>,
    pub class: Option<u32>,
    pub tx_power: Option<i16>,
    pub manuf: Vec<(u16, Vec<u8>)>,
    pub svc_data: Vec<(String, Vec<u8>)>,
    pub uuids: Vec<String>,
    pub paired: bool,
    pub bonded: bool,
    pub connected: bool,
    pub trusted: bool,
    pub blocked: bool,
    /// (mono ms, dBm)
    pub rssi: VecDeque<(u64, i16)>,
    /// mono ms / wall clock (unix s) of the first and last RSSI report
    pub first_seen: u64,
    pub last_seen: u64,
    pub first_wall: i64,
    pub last_wall: i64,
    /// BlueZ dropped the device (not heard for its TemporaryTimeout)
    pub gone: bool,
}

impl Dev {
    pub fn seen(&self) -> bool {
        self.first_seen > 0
    }

    /// Median of the last five samples of the last `window` ms (falls back
    /// to the newest sample). BlueZ only reports an RSSI when it changed, so
    /// samples come every few seconds.
    pub fn rssi_smooth(&self, now: u64, window: u64) -> Option<i16> {
        let mut v: Vec<i16> = self.rssi.iter().rev().take(5).filter(|(t, _)| now.saturating_sub(*t) <= window).map(|s| s.1).collect();
        if v.is_empty() {
            return self.rssi.back().map(|s| s.1);
        }
        v.sort_unstable();
        Some(v[v.len() / 2])
    }
}

#[derive(Default)]
struct Shared {
    status: Option<Status>,
    devs: HashMap<String, Dev>,
    conn: Option<Connection>,
    /// device list changed since the last snapshot
    dirty: bool,
}

pub struct Scanner {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl Scanner {
    pub fn start() -> Scanner {
        let shared = Arc::new(Mutex::new(Shared { status: Some(Status::Starting), ..Default::default() }));
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (sh, st, dn) = (shared.clone(), stop.clone(), done.clone());
        std::thread::Builder::new()
            .name("btscan".into())
            .spawn(move || {
                let r = run(&sh, &st);
                if let Ok(mut g) = sh.lock() {
                    if let Err(e) = r {
                        g.status = Some(e);
                    }
                    g.conn = None;
                }
                dn.store(true, Ordering::Relaxed);
            })
            .ok();
        Scanner { shared, stop, done }
    }

    pub fn status(&self) -> Status {
        self.shared.lock().ok().and_then(|g| g.status.clone()).unwrap_or(Status::Starting)
    }

    /// The scanner thread has ended (error or no bluetoothd): start a new one
    /// to retry.
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// Copy of the devices that reported an RSSI during this scan.
    pub fn snapshot(&self) -> Vec<Dev> {
        let Ok(mut g) = self.shared.lock() else { return Vec::new() };
        g.dirty = false;
        g.devs.values().filter(|d| d.seen()).cloned().collect()
    }

    pub fn device(&self, path: &str) -> Option<Dev> {
        self.shared.lock().ok()?.devs.get(path).cloned()
    }

    /// Ask the scanner thread to stop discovery and end. Does not block.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let conn = self.shared.lock().ok().and_then(|g| g.conn.clone());
        if let Some(c) = conn {
            // wake the thread: a signal addressed to ourselves
            if let Some(me) = c.unique_name().map(|n| n.to_string()) {
                let _ = c.emit_signal(Some(me.as_str()), "/de/pocketchip/radar", OWN_IFACE, "Stop", &());
            }
        }
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wall_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn set_status(sh: &Mutex<Shared>, s: Status) {
    if let Ok(mut g) = sh.lock() {
        g.status = Some(s);
    }
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn run(sh: &Mutex<Shared>, stop: &AtomicBool) -> Result<(), Status> {
    let conn = Connection::system().map_err(|e| Status::Error(format!("D-BUS: {e}")))?;
    conn.clone().set_max_queued(512);
    if let Ok(mut g) = sh.lock() {
        g.conn = Some(conn.clone());
    }
    let dbus = zbus::blocking::fdo::DBusProxy::new(&conn).map_err(|e| Status::Error(e.to_string()))?;
    let rules = [
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .sender("org.bluez")
            .and_then(|b| b.interface("org.freedesktop.DBus.ObjectManager"))
            .map(|b| b.build()),
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .sender("org.bluez")
            .and_then(|b| b.interface("org.freedesktop.DBus.Properties"))
            .and_then(|b| b.member("PropertiesChanged"))
            .and_then(|b| b.path_namespace("/org/bluez"))
            .map(|b| b.build()),
    ];
    for r in rules {
        let r = r.map_err(|e| Status::Error(e.to_string()))?;
        dbus.add_match_rule(r).map_err(|e| Status::Error(e.to_string()))?;
    }
    // subscribe before reading the object tree, so nothing is missed
    let iter = MessageIterator::from(&conn);

    let objs: Objects = conn
        .call_method(Some("org.bluez"), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .map_err(|_| Status::NoService)?
        .body()
        .deserialize()
        .map_err(|e| Status::Error(e.to_string()))?;

    let mut adapter: Option<String> = None;
    let mut powered = false;
    let mut paths: Vec<_> = objs.keys().map(|p| p.to_string()).collect();
    paths.sort();
    for p in &paths {
        let ifs = &objs[&OwnedObjectPath::try_from(p.as_str()).unwrap()];
        if adapter.is_none() {
            if let Some(a) = ifs.get(ADAPTER) {
                adapter = Some(p.clone());
                powered = a.get("Powered").and_then(as_bool).unwrap_or(false);
            }
        }
    }
    if let Ok(mut g) = sh.lock() {
        for (p, ifs) in &objs {
            if let Some(props) = ifs.get(DEVICE) {
                let d = g.devs.entry(p.to_string()).or_insert_with(|| Dev { path: p.to_string(), ..Default::default() });
                // cached devices: properties yes, but no RSSI unless seen now
                apply(d, props, false);
            }
        }
    }

    let mut want_scan = false;
    let mut last_start = 0u64;
    let start = |adapter: &str, last_start: &mut u64| -> Result<(), String> {
        *last_start = mono_ms();
        let mut f: HashMap<&str, Value> = HashMap::new();
        f.insert("Transport", Value::from("le"));
        f.insert("DuplicateData", Value::from(true));
        conn.call_method(Some("org.bluez"), adapter, Some(ADAPTER), "SetDiscoveryFilter", &(f,)).map_err(|e| e.to_string())?;
        conn.call_method(Some("org.bluez"), adapter, Some(ADAPTER), "StartDiscovery", &()).map_err(|e| e.to_string())?;
        Ok(())
    };
    match &adapter {
        None => set_status(sh, Status::NoAdapter),
        Some(_) if !powered => set_status(sh, Status::BtOff),
        Some(a) => match start(a, &mut last_start) {
            Ok(()) => {
                want_scan = true;
                set_status(sh, Status::Scanning)
            }
            Err(e) => set_status(sh, Status::Error(crate::util::short_err(&e))),
        },
    }

    for msg in iter {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(msg) = msg else { continue };
        if msg.message_type() != MsgType::Signal {
            continue;
        }
        let hdr = msg.header();
        let (Some(iface), Some(member)) = (hdr.interface(), hdr.member()) else { continue };
        let path = hdr.path().map(|p| p.to_string()).unwrap_or_default();
        match (iface.as_str(), member.as_str()) {
            (OWN_IFACE, _) => break,
            ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
                let Ok((ifname, changed, _inv)) = msg.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() else {
                    continue;
                };
                if ifname == DEVICE {
                    if let Ok(mut g) = sh.lock() {
                        let d = g.devs.entry(path.clone()).or_insert_with(|| Dev { path: path.clone(), ..Default::default() });
                        apply(d, &changed, true);
                        g.dirty = true;
                    }
                } else if ifname == ADAPTER && adapter.as_deref() == Some(path.as_str()) {
                    if let Some(p) = changed.get("Powered").and_then(as_bool) {
                        powered = p;
                        if !p {
                            want_scan = false;
                            set_status(sh, Status::BtOff);
                        }
                    }
                    let lost = changed.get("Discovering").and_then(as_bool) == Some(false);
                    if powered && (!want_scan || lost) && mono_ms() > last_start + 3000 {
                        match start(&path, &mut last_start) {
                            Ok(()) => {
                                want_scan = true;
                                set_status(sh, Status::Scanning)
                            }
                            Err(e) => set_status(sh, Status::Error(crate::util::short_err(&e))),
                        }
                    }
                }
            }
            ("org.freedesktop.DBus.ObjectManager", "InterfacesAdded") => {
                let Ok((p, ifs)) = msg.body().deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>() else {
                    continue;
                };
                let p = p.to_string();
                if let Some(props) = ifs.get(DEVICE) {
                    if let Ok(mut g) = sh.lock() {
                        let d = g.devs.entry(p.clone()).or_insert_with(|| Dev { path: p.clone(), ..Default::default() });
                        apply(d, props, true);
                        g.dirty = true;
                    }
                }
                if let (None, Some(a)) = (&adapter, ifs.get(ADAPTER)) {
                    adapter = Some(p.clone());
                    powered = a.get("Powered").and_then(as_bool).unwrap_or(false);
                    if powered {
                        match start(&p, &mut last_start) {
                            Ok(()) => {
                                want_scan = true;
                                set_status(sh, Status::Scanning)
                            }
                            Err(e) => set_status(sh, Status::Error(crate::util::short_err(&e))),
                        }
                    } else {
                        set_status(sh, Status::BtOff);
                    }
                }
            }
            ("org.freedesktop.DBus.ObjectManager", "InterfacesRemoved") => {
                let Ok((p, ifs)) = msg.body().deserialize::<(OwnedObjectPath, Vec<String>)>() else { continue };
                if adapter.as_deref() == Some(p.as_str()) && ifs.iter().any(|i| i == ADAPTER) {
                    adapter = None;
                    want_scan = false;
                    set_status(sh, Status::NoAdapter);
                }
                if ifs.iter().any(|i| i == DEVICE) {
                    if let Ok(mut g) = sh.lock() {
                        if let Some(d) = g.devs.get_mut(p.as_str()) {
                            d.gone = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let (Some(a), true) = (&adapter, want_scan) {
        let _ = conn.call_method(Some("org.bluez"), a.as_str(), Some(ADAPTER), "StopDiscovery", &());
    }
    Ok(())
}

// ---- property decoding --------------------------------------------------------

fn inner<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v {
        Value::Value(b) => inner(b),
        _ => v,
    }
}

fn as_bool(v: &OwnedValue) -> Option<bool> {
    match inner(v) {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn as_str(v: &OwnedValue) -> Option<String> {
    match inner(v) {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn as_i64(v: &Value) -> Option<i64> {
    Some(match inner(v) {
        Value::U8(n) => *n as i64,
        Value::I16(n) => *n as i64,
        Value::U16(n) => *n as i64,
        Value::I32(n) => *n as i64,
        Value::U32(n) => *n as i64,
        Value::I64(n) => *n,
        _ => return None,
    })
}

fn as_bytes(v: &Value) -> Vec<u8> {
    match inner(v) {
        Value::Array(a) => a.iter().filter_map(|b| as_i64(b).map(|n| n as u8)).collect(),
        _ => Vec::new(),
    }
}

fn apply(d: &mut Dev, props: &HashMap<String, OwnedValue>, live: bool) {
    for (k, v) in props {
        match k.as_str() {
            "Address" => d.addr = as_str(v).unwrap_or_default(),
            "AddressType" => d.addr_type = as_str(v).unwrap_or_default(),
            "Name" => d.name = as_str(v).unwrap_or_default(),
            "Alias" => d.alias = as_str(v).unwrap_or_default(),
            "Icon" => d.icon = as_str(v).unwrap_or_default(),
            "Appearance" => d.appearance = as_i64(v).map(|n| n as u16),
            "Class" => d.class = as_i64(v).map(|n| n as u32),
            "TxPower" => d.tx_power = as_i64(v).map(|n| n as i16),
            "Paired" => d.paired = as_bool(v).unwrap_or(false),
            "Bonded" => d.bonded = as_bool(v).unwrap_or(false),
            "Connected" => d.connected = as_bool(v).unwrap_or(false),
            "Trusted" => d.trusted = as_bool(v).unwrap_or(false),
            "Blocked" => d.blocked = as_bool(v).unwrap_or(false),
            "UUIDs" => {
                if let Value::Array(a) = inner(v) {
                    d.uuids = a.iter().filter_map(|u| if let Value::Str(s) = inner(u) { Some(s.to_string()) } else { None }).collect();
                }
            }
            "ManufacturerData" => {
                if let Value::Dict(m) = inner(v) {
                    d.manuf = m.iter().filter_map(|(k, v)| Some((as_i64(k)? as u16, as_bytes(v)))).collect();
                    d.manuf.sort_by_key(|m| m.0);
                }
            }
            "ServiceData" => {
                if let Value::Dict(m) = inner(v) {
                    d.svc_data = m
                        .iter()
                        .filter_map(|(k, v)| if let Value::Str(s) = inner(k) { Some((s.to_string(), as_bytes(v))) } else { None })
                        .collect();
                    d.svc_data.sort();
                }
            }
            "RSSI" if live => {
                if let Some(r) = as_i64(v) {
                    let now = mono_ms();
                    d.gone = false;
                    if d.first_seen == 0 {
                        d.first_seen = now;
                        d.first_wall = wall_now();
                    }
                    d.last_seen = now;
                    d.last_wall = wall_now();
                    d.rssi.push_back((now, r as i16));
                    while d.rssi.len() > HISTORY {
                        d.rssi.pop_front();
                    }
                }
            }
            _ => {}
        }
    }
}
