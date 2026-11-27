mod audio;
mod bt;
mod debug;
mod hw;
mod led;
mod settings;
mod sys;
mod timecfg;
mod touchcal;
mod util;
mod wifi;

use settings::{fmt_timeout, step, Settings, SCREEN_STEPS, STANDBY_STEPS};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;
use util::{bar, fmt_duration, mono_ms};

slint::include_modules!();

#[derive(Clone, Debug, PartialEq)]
enum Page {
    Home,
    Menu,
    Status,
    Battery,
    Power,
    Display,
    Audio,
    Wifi,
    Bt,
    Time,
    Emus,
    /// Pause menu over a running emulator (`--overlay LABEL`).
    Overlay(String),
    TzRegion,
    TzCity(String),
    BtDevice(String),
}

impl Page {
    fn title(&self) -> String {
        match self {
            Page::Home => "START".into(),
            Page::Menu => "EINSTELLUNGEN".into(),
            Page::Status => "SYSTEMSTATUS".into(),
            Page::Battery => "AKKU / LADUNG".into(),
            Page::Power => "ENERGIE".into(),
            Page::Display => "ANZEIGE".into(),
            Page::Audio => "AUDIO".into(),
            Page::Wifi => "WLAN".into(),
            Page::Bt => "BLUETOOTH".into(),
            Page::Time => "DATUM / ZEIT".into(),
            Page::Emus => "EMULATOREN".into(),
            Page::Overlay(l) => format!("PAUSE / {l}"),
            Page::TzRegion => "ZEITZONE".into(),
            Page::TzCity(r) => format!("ZEITZONE / {}", r.to_uppercase()),
            Page::BtDevice(_) => "BLUETOOTH / GERÄT".into(),
        }
    }
}

// ---- row helpers -----------------------------------------------------------

fn row(id: &str, label: &str, value: &str, kind: i32, enabled: bool) -> Row {
    Row { id: id.into(), label: label.into(), value: value.into(), kind, enabled }
}
fn action(id: &str, label: &str, value: &str) -> Row {
    row(id, label, value, 0, true)
}
fn toggle(id: &str, label: &str, on: bool, enabled: bool) -> Row {
    row(id, label, if on { "AN" } else { "AUS" }, 1, enabled)
}
fn adjust(id: &str, label: &str, value: &str) -> Row {
    row(id, label, value, 2, true)
}
fn info(label: &str, value: &str) -> Row {
    row("", label, value, 3, true)
}
fn sep(label: &str) -> Row {
    row("", label, "", 4, false)
}

/// Emulators started full screen by pocketchip-emu@<id>.service:
/// (id, label, program that must be installed, games folder under $HOME).
const EMUS: &[(&str, &str, &str, &str)] = &[("scummvm", "SCUMMVM", "/usr/games/scummvm", "~/games/scummvm")];

struct App {
    ui: AppWindow,
    rows: Rc<VecModel<Row>>,
    stack: Vec<(Page, i32)>,
    settings: Settings,
    backlight: sys::Backlight,
    cpu: sys::CpuSampler,
    bat: sys::Battery,
    ips: Vec<(String, String)>,
    wifi: wifi::WifiState,
    nets: Vec<wifi::Network>,
    wifi_busy: Option<String>,
    bt: bt::BtState,
    bt_devs: Vec<bt::Device>,
    bt_busy: Option<String>,
    bt_dev: Option<bt::DevInfo>,
    time: timecfg::TimeState,
    edit: timecfg::Local,
    zones: Vec<String>,
    dialog_actions: Vec<String>,
    screen_off: bool,
    suspending_until: u64,
    ignore_power_until: u64,
    last_suspended: u64,
    msg_until: u64,
    tick_n: u64,
    restore_sel: bool,
    calib: Vec<(f64, f64)>,
    calib_result: Option<[f64; 6]>,
    standby: bool,
}

/// Exit code of the pause menu: 0 resume, 10 quit the emulator.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
const EXIT_QUIT_EMU: i32 = 10;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Run `f` with the app state; defers if the state is already borrowed.
fn with_app(f: impl FnOnce(&mut App) + 'static) {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => {
            if let Some(app) = g.as_mut() {
                f(app)
            }
        }
        Err(_) => {
            slint::Timer::single_shot(Duration::ZERO, move || with_app(f));
        }
    });
}

/// Run blocking work on a thread, then handle the result on the UI thread.
fn bg<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&mut App, T) + Send + 'static) {
    std::thread::spawn(move || {
        let r = work();
        let _ = slint::invoke_from_event_loop(move || with_app(move |a| done(a, r)));
    });
}

impl App {
    fn be(&self) -> Backend<'_> {
        self.ui.global::<Backend>()
    }

    fn page(&self) -> Page {
        self.stack.last().map(|p| p.0.clone()).unwrap_or(Page::Home)
    }

    fn message(&mut self, text: &str, secs: u64) {
        self.be().set_message(text.into());
        self.msg_until = mono_ms() + secs * 1000;
    }

    // ---- navigation -------------------------------------------------------

    fn push(&mut self, p: Page) {
        let sel = self.be().get_sel();
        if let Some(top) = self.stack.last_mut() {
            top.1 = sel;
        }
        self.stack.push((p.clone(), -1));
        self.restore_sel = true;
        self.on_enter(&p);
        self.refresh();
    }

    fn back(&mut self) {
        if self.be().get_dialog_open() || self.be().get_input_open() {
            return;
        }
        if self.stack.len() > 1 {
            self.stack.pop();
            self.restore_sel = true;
            self.refresh();
        } else if self.overlay().is_some() {
            self.overlay_exit(0);
        }
    }

    fn on_enter(&mut self, p: &Page) {
        match p {
            Page::Wifi => {
                self.poll_wifi();
                self.scan_wifi(false);
            }
            Page::Bt => self.poll_bt(true),
            Page::BtDevice(mac) => {
                self.bt_dev = None;
                self.poll_bt_dev(mac.clone());
            }
            Page::Time => {
                self.edit = timecfg::now();
                self.poll_time();
            }
            Page::TzRegion | Page::TzCity(_) => {
                if self.zones.is_empty() {
                    self.zones = timecfg::zones();
                }
            }
            Page::Status => {
                self.ips = sys::ip_addrs();
            }
            _ => {}
        }
    }

    /// Rebuild the rows of the current page and push them to the UI.
    fn refresh(&mut self) {
        let page = self.page();
        let rows = self.build_rows(&page);
        let ui = self.ui.clone_strong();
        let be = ui.global::<Backend>();
        be.set_title(page.title().into());
        be.set_depth((self.stack.len() - 1) as i32);
        be.set_home(page == Page::Home);
        if self.rows.row_count() == rows.len() {
            for (i, r) in rows.iter().enumerate() {
                if self.rows.row_data(i).as_ref() != Some(r) {
                    self.rows.set_row_data(i, r.clone());
                }
            }
        } else {
            self.rows.set_vec(rows.clone());
        }
        // restore or pick the selection
        let want = if self.restore_sel { self.stack.last().map(|p| p.1).unwrap_or(-1) } else { be.get_sel() };
        self.restore_sel = false;
        let sel_ok = |s: i32| rows.get(s as usize).map(|r| r.kind != 4 && r.enabled).unwrap_or(false);
        let mut sel = want;
        if want < 0 {
            sel = self.default_sel(&page, &rows);
        } else if !sel_ok(sel) {
            // keep the cursor near where it was when rows change
            sel = (sel..rows.len() as i32).chain((0..sel).rev()).find(|&i| sel_ok(i)).unwrap_or(0);
        }
        if sel as usize >= rows.len() || rows.get(sel as usize).map(|r| r.kind == 4 || !r.enabled).unwrap_or(true) {
            sel = rows.iter().position(|r| r.kind != 4 && r.enabled).unwrap_or(0) as i32;
        }
        if let Some(top) = self.stack.last_mut() {
            top.1 = sel;
        }
        be.set_sel(sel);
    }

    fn default_sel(&self, page: &Page, rows: &[Row]) -> i32 {
        let find = |id: &str| rows.iter().position(|r| r.id == id).map(|p| p as i32);
        match page {
            Page::TzRegion => {
                let region = self.time.timezone.split('/').next().unwrap_or("");
                find(&format!("r:{region}")).unwrap_or(0)
            }
            Page::TzCity(_) => find(&format!("z:{}", self.time.timezone)).unwrap_or(0),
            _ => 0,
        }
    }

    fn build_rows(&self, page: &Page) -> Vec<Row> {
        match page {
            Page::Home => vec![action("settings", "EINSTELLUNGEN", ""), action("emus", "EMULATOREN", "")],
            Page::Emus => self.rows_emus(),
            Page::Overlay(l) => self.rows_overlay(l),
            Page::Menu => self.rows_menu(),
            Page::Status => self.rows_status(),
            Page::Battery => self.rows_battery(),
            Page::Power => vec![
                action("poweroff", "AUSSCHALTEN", ""),
                action("reboot", "NEUSTART", ""),
                action("suspend", "STANDBY", ""),
                sep("HINWEIS"),
                info("POWER-TASTE", "ÖFFNET ABFRAGE"),
                info("LANGER DRUCK 6 S", "HARTES AUS"),
            ],
            Page::Display => {
                let s = &self.settings;
                vec![
                    adjust("bright", "HELLIGKEIT", &format!("{} {:>2}", bar(s.brightness * 100 / self.backlight.max, 10), s.brightness)),
                    adjust("screen", "DISPLAY AUS NACH", &fmt_timeout(s.screen_timeout)),
                    adjust("standby", "STANDBY NACH", &fmt_timeout(s.standby_timeout)),
                    toggle("standby_bat", "STANDBY NUR AKKU", s.standby_on_battery_only, true),
                    toggle("scanlines", "SCANLINES", s.scanlines, true),
                    toggle("led_on", "LED IM BETRIEB", s.led_on, true),
                    row("led_standby", "LED IM STANDBY", if s.led_standby_blink { "BLINKEN" } else { "AUS" }, 1, true),
                    action("screen_now", "DISPLAY JETZT AUS", ""),
                    action("touchcal", "TOUCH KALIBRIEREN", ""),
                ]
            }
            Page::Audio => {
                let s = &self.settings;
                vec![
                    adjust("vol", "LAUTSTÄRKE", &format!("{} {:>3}%", bar(s.volume, 10), s.volume)),
                    toggle("mute", "STUMM", s.muted, true),
                    action("tone", "TESTTON", ""),
                    sep("HINWEIS"),
                    info("AUSGANG", "KLINKE 3,5 MM"),
                    info("LAUTSPRECHER", "NICHT EINGEBAUT"),
                ]
            }
            Page::Wifi => self.rows_wifi(),
            Page::Bt => self.rows_bt(),
            Page::BtDevice(mac) => self.rows_bt_dev(mac),
            Page::Time => self.rows_time(),
            Page::TzRegion => {
                let mut regions: Vec<String> = Vec::new();
                for z in &self.zones {
                    let r = z.split('/').next().unwrap_or(z).to_string();
                    if !regions.contains(&r) {
                        regions.push(r);
                    }
                }
                regions
                    .iter()
                    .map(|r| {
                        let n = self.zones.iter().filter(|z| z.starts_with(&format!("{r}/"))).count();
                        action(&format!("r:{r}"), &r.to_uppercase(), &if n > 0 { format!("{n} »") } else { String::new() })
                    })
                    .collect()
            }
            Page::TzCity(region) => {
                let pre = format!("{region}/");
                self.zones
                    .iter()
                    .filter(|z| z.starts_with(&pre))
                    .map(|z| {
                        let cur = if *z == self.time.timezone { "●" } else { "" };
                        action(&format!("z:{z}"), &z[pre.len()..].replace('_', " ").to_uppercase(), cur)
                    })
                    .collect()
            }
        }
    }

    fn rows_menu(&self) -> Vec<Row> {
        let now = timecfg::now();
        let wifi = if !self.wifi.radio {
            "AUS".to_string()
        } else {
            self.wifi.ssid.clone().unwrap_or_else(|| "GETRENNT".into())
        };
        let bt = if !self.bt.service {
            "DIENST AUS"
        } else if !self.bt.adapter {
            "KEIN ADAPTER"
        } else if self.bt.powered && !self.bt.blocked {
            "AN"
        } else {
            "AUS"
        };
        vec![
            action("status", "01 SYSTEMSTATUS", &format!("CPU {:.0}%", self.cpu.load)),
            action("battery", "02 AKKU", &format!("{} {}", self.bat.short(), self.bat.status_de())),
            action("power", "03 ENERGIE", "AUS/NEU/STANDBY"),
            action("display", "04 ANZEIGE", &format!("HELL {}", self.settings.brightness)),
            action("audio", "05 AUDIO", &if self.settings.muted { "STUMM".to_string() } else { format!("LAUT {}%", self.settings.volume) }),
            action("wifi", "06 WLAN", &wifi),
            action("bt", "07 BLUETOOTH", bt),
            action("time", "08 DATUM / ZEIT", &format!("{:02}.{:02}. {:02}:{:02}", now.day, now.month, now.hour, now.minute)),
        ]
    }

    fn rows_emus(&self) -> Vec<Row> {
        let mut v = Vec::new();
        for (id, label, prog, games) in EMUS {
            let ok = std::path::Path::new(prog).exists();
            v.push(row(&format!("emu:{id}"), label, if ok { "STARTEN" } else { "NICHT INSTALLIERT" }, 0, ok));
            v.push(info("  SPIELE", games));
        }
        v.push(sep("BEDIENUNG"));
        v.push(info("BEENDEN", "STRG+Q"));
        v.push(info("SCUMMVM-MENÜ", "STRG+F5"));
        v
    }

    fn rows_overlay(&self, label: &str) -> Vec<Row> {
        let s = sys::SysInfo::read();
        let st = &self.settings;
        let now = timecfg::now();
        let wifi = if !self.wifi.radio {
            "AUS".to_string()
        } else {
            self.wifi.ssid.clone().unwrap_or_else(|| "GETRENNT".into())
        };
        let mut v = vec![
            action("resume", "WEITER SPIELEN", ""),
            action("quit_emu", &format!("{label} BEENDEN"), ""),
            sep("STATUS"),
            info("AKKU", &format!("{} {}", self.bat.short(), self.bat.status_de())),
            info("UHRZEIT", &format!("{:02}.{:02}.{} {:02}:{:02}", now.day, now.month, now.year, now.hour, now.minute)),
            info("WLAN", &wifi),
        ];
        if let Some((_, ip)) = self.ips.first() {
            v.push(info("IP", ip));
        }
        let temp = s.soc_temp.map(|t| format!("  {t:.0} °C")).unwrap_or_default();
        v.push(info("CPU", &format!("{:.0}%{temp}", self.cpu.load)));
        v.push(info("RAM", &format!("{} / {} MB", (s.mem_total_kb - s.mem_avail_kb) / 1024, s.mem_total_kb / 1024)));
        v.push(sep("EINSTELLUNGEN"));
        v.push(adjust("bright", "HELLIGKEIT", &format!("{} {:>2}", bar(st.brightness * 100 / self.backlight.max, 10), st.brightness)));
        v.push(adjust("vol", "LAUTSTÄRKE", &format!("{} {:>3}%", bar(st.volume, 10), st.volume)));
        v.push(toggle("mute", "STUMM", st.muted, true));
        v.push(sep("ENERGIE"));
        v.push(action("suspend", "STANDBY", ""));
        v.push(action("reboot", "NEUSTART", ""));
        v.push(action("poweroff", "AUSSCHALTEN", ""));
        v
    }

    /// Leave the pause menu with an exit code for pocketchip-emu-helper.
    fn overlay_exit(&mut self, code: i32) {
        EXIT_CODE.store(code, Ordering::Relaxed);
        let _ = slint::quit_event_loop();
    }

    fn overlay(&self) -> Option<String> {
        match self.stack.first() {
            Some((Page::Overlay(l), _)) => Some(l.clone()),
            _ => None,
        }
    }

    fn rows_status(&self) -> Vec<Row> {
        let s = sys::SysInfo::read();
        let used = (s.mem_total_kb - s.mem_avail_kb) / 1024;
        let mut v = vec![
            info("HOST", &s.hostname.to_uppercase()),
            info("MODELL", &s.model),
            info("KERNEL", &s.kernel),
            info("CPU", &s.cpu_model),
            info("TAKT", &format!("{} MHZ", s.cpu_mhz)),
            info("AUSLASTUNG", &format!("{} {:>3.0}%", bar(self.cpu.load as i64, 10), self.cpu.load)),
            info("LOAD", &s.loadavg),
            info("RAM", &format!("{} / {} MB", used, s.mem_total_kb / 1024)),
            info("SWAP", &format!("{} / {} MB", (s.swap_total_kb - s.swap_free_kb) / 1024, s.swap_total_kb / 1024)),
            info("SPEICHER /", &format!("{} FREI / {}", sys::fmt_bytes(s.disk_free), sys::fmt_bytes(s.disk_total))),
            info("UPTIME", &fmt_duration(s.uptime_s)),
        ];
        if self.ips.is_empty() {
            v.push(info("IP", "KEINE"));
        }
        for (dev, ip) in &self.ips {
            v.push(info(&format!("IP {}", dev.to_uppercase()), ip));
        }
        if let Some(t) = s.soc_temp {
            v.push(info("TEMP SOC", &format!("{t:.1} °C")));
        }
        if let Some(t) = s.pmic_temp {
            v.push(info("TEMP PMIC", &format!("{t:.1} °C")));
        }
        v
    }

    fn rows_battery(&self) -> Vec<Row> {
        let b = &self.bat;
        let mut v = Vec::new();
        if b.present {
            let p = b.voltage_uv as f64 / 1e6 * b.current_ua as f64 / 1e6;
            v.push(info("LADUNG", &format!("{} {:>3}%", bar(b.capacity, 10), b.capacity)));
            v.push(info("STATUS", b.status_de()));
            v.push(info("SPANNUNG", &format!("{:.3} V", b.voltage_uv as f64 / 1e6)));
            v.push(info("STROM", &format!("{} MA", b.current_ua / 1000)));
            v.push(info("LEISTUNG", &format!("{:.2} W", p.abs())));
            v.push(info("ZUSTAND", match b.health.as_str() { "Good" => "GUT", "Overheat" => "ÜBERHITZT", "Dead" => "DEFEKT", "Over voltage" => "ÜBERSPANNUNG", h => h }));
        } else {
            v.push(info("AKKU", "NICHT VORHANDEN"));
        }
        v.push(sep("EINGANG"));
        v.push(info(
            "USB-EINGANG",
            &if b.usb_online {
                format!("JA {:.2} V {} MA", b.usb_voltage_uv as f64 / 1e6, b.usb_current_ua / 1000)
            } else {
                "NEIN".into()
            },
        ));
        // ACIN is not wired on the PocketCHIP; only show it when it carries power
        if b.ac_online {
            v.push(info("DC-EINGANG (ACIN)", "JA"));
        }
        v
    }

    fn rows_wifi(&self) -> Vec<Row> {
        let w = &self.wifi;
        let connected = w.ssid.is_some();
        let ip = self
            .ips
            .iter()
            .find(|(d, _)| Some(d) == w.device.as_ref())
            .map(|(_, ip)| ip.clone())
            .unwrap_or_else(|| "—".into());
        let mut v = vec![
            toggle("radio", "WLAN-FUNK", w.radio, true),
            info("VERBUNDEN", w.ssid.as_deref().unwrap_or("—")),
            info("IP-ADRESSE", &ip),
            row("disconnect", "TRENNEN", "", 0, connected),
            row("scan", "NETZE SUCHEN", self.wifi_busy.as_deref().unwrap_or(""), 0, w.radio && self.wifi_busy.is_none()),
            sep(&format!("NETZE ({})", self.nets.len())),
        ];
        for (i, n) in self.nets.iter().enumerate() {
            let sec = if n.security.is_empty() || n.security == "--" { "OFFEN".to_string() } else { n.security.replace("WPA1 ", "").to_uppercase() };
            let mark = if n.in_use { " ●" } else if n.known { " +" } else { "" };
            v.push(row(&format!("net:{i}"), &n.ssid, &format!("{} {}{}", wifi::signal_bars(n.signal), sec, mark), 0, self.wifi_busy.is_none()));
        }
        v
    }

    fn rows_bt(&self) -> Vec<Row> {
        let b = &self.bt;
        let mut v = Vec::new();
        if !b.service {
            v.push(info("DIENST", "GESTOPPT"));
            v.push(action("bt_service", "DIENST STARTEN", ""));
        }
        if b.service && !b.adapter {
            v.push(info("ADAPTER", "NICHT GEFUNDEN"));
        }
        let on = b.powered && !b.blocked;
        // stays selectable while busy so the cursor does not jump away
        v.push(toggle("bt_power", "BLUETOOTH", on, b.adapter));
        if b.adapter {
            v.push(info("NAME", &b.name));
        }
        v.push(row("bt_scan", "GERÄTE SUCHEN", self.bt_busy.as_deref().unwrap_or(""), 0, on && self.bt_busy.is_none()));
        v.push(sep(&format!("GERÄTE ({})", self.bt_devs.len())));
        for (i, d) in self.bt_devs.iter().enumerate() {
            let st = if d.connected { "VERBUNDEN" } else if d.paired { "GEKOPPELT" } else { "" };
            v.push(row(&format!("dev:{i}"), &d.name, st, 0, self.bt_busy.is_none()));
        }
        v
    }

    fn rows_bt_dev(&self, mac: &str) -> Vec<Row> {
        let Some(d) = self.bt_dev.as_ref().filter(|d| d.mac == mac) else {
            return vec![info("ADRESSE", mac), info("STATUS", "LESE ...")];
        };
        let yn = |b: bool| if b { "JA" } else { "NEIN" };
        let free = self.bt_busy.is_none();
        let mut v = vec![
            info("NAME", &d.name),
            info("ADRESSE", &d.mac),
            info("TYP", bt::icon_de(&d.icon)),
            info("GEKOPPELT", yn(d.paired)),
            info("VERBUNDEN", yn(d.connected)),
            info("VERTRAUT", yn(d.trusted)),
        ];
        if let Some(r) = d.rssi {
            let q = ((r + 100) * 100 / 60).clamp(0, 100) as i64;
            v.push(info("SIGNAL", &format!("{} {r} DBM", bar(q, 6))));
        }
        if let Some(b) = d.battery {
            v.push(info("AKKU", &format!("{b}%")));
        }
        if d.blocked {
            v.push(info("GESPERRT", "JA"));
        }
        v.push(sep("AKTIONEN"));
        if let Some(b) = &self.bt_busy {
            v.push(info("STATUS", b));
        }
        if d.connected {
            v.push(row("bd_disconnect", "TRENNEN", "", 0, free));
        } else if d.paired {
            v.push(row("bd_connect", "VERBINDEN", "", 0, free));
        } else {
            v.push(row("bd_pair", "KOPPELN + VERBINDEN", "", 0, free));
        }
        if d.paired || d.trusted {
            v.push(row("bd_remove", "ENTFERNEN", "", 0, free));
        }
        if !d.services.is_empty() {
            v.push(sep(&format!("DIENSTE ({})", d.services.len())));
            for svc in &d.services {
                v.push(info("", &svc.to_uppercase()));
            }
        }
        v
    }

    fn rows_time(&self) -> Vec<Row> {
        let n = timecfg::now();
        let e = &self.edit;
        vec![
            info("DATUM", &format!("{} {:02}.{:02}.{:04}", timecfg::WEEKDAYS[n.wday as usize % 7], n.day, n.month, n.year)),
            info("UHRZEIT", &format!("{:02}:{:02}:{:02}", n.hour, n.minute, n.second)),
            toggle("ntp", "NETZWERKZEIT", self.time.ntp, true),
            info("SYNCHRON", if self.time.synced { "JA" } else { "NEIN" }),
            action("tz", "ZEITZONE", &self.time.timezone),
            sep("MANUELL STELLEN"),
            adjust("y", "JAHR", &format!("{:04}", e.year)),
            adjust("mo", "MONAT", &format!("{:02}", e.month)),
            adjust("d", "TAG", &format!("{:02}", e.day)),
            adjust("h", "STUNDE", &format!("{:02}", e.hour)),
            adjust("mi", "MINUTE", &format!("{:02}", e.minute)),
            action("settime", "ZEIT SETZEN", ""),
        ]
    }

    // ---- dialogs ------------------------------------------------------------

    fn dialog(&mut self, id: &str, title: &str, text: &str, buttons: &[(&str, &str)], default: i32) {
        let ui = self.ui.clone_strong();
        let be = ui.global::<Backend>();
        be.set_dialog_id(id.into());
        be.set_dialog_title(title.into());
        be.set_dialog_text(text.into());
        let labels: Vec<SharedString> = buttons.iter().map(|b| SharedString::from(b.0)).collect();
        self.dialog_actions = buttons.iter().map(|b| b.1.to_string()).collect();
        be.set_dialog_buttons(ModelRc::new(VecModel::from(labels)));
        be.set_dialog_sel(default);
        be.set_dialog_open(true);
    }

    fn confirm(&mut self, id: &str, title: &str, text: &str) {
        self.dialog(id, title, text, &[("JA", "yes"), ("NEIN", "no")], 1);
    }

    fn power_menu(&mut self) {
        self.dialog(
            "power",
            "ENERGIE",
            "SYSTEM AUSSCHALTEN?",
            &[("AUSSCHALTEN", "poweroff"), ("NEUSTART", "reboot"), ("STANDBY", "suspend"), ("ABBRECHEN", "no")],
            3,
        );
    }

    fn dialog_result(&mut self, id: String, idx: i32) {
        self.be().set_dialog_open(false);
        let act = if idx >= 0 { self.dialog_actions.get(idx as usize).cloned().unwrap_or_default() } else { "no".into() };
        self.ui.invoke_refocus();
        if act == "no" || act.is_empty() {
            return;
        }
        let (kind, arg) = id.split_once(':').unwrap_or((id.as_str(), ""));
        let arg = arg.to_string();
        match (kind, act.as_str()) {
            ("power", a) => self.do_power(a),
            ("poweroff", "yes") => self.do_power("poweroff"),
            ("reboot", "yes") => self.do_power("reboot"),
            ("suspend", "yes") => self.do_power("suspend"),
            ("quit_emu", "yes") => self.overlay_exit(EXIT_QUIT_EMU),
            ("settime", "yes") => {
                let t = self.edit;
                self.message("SETZE ZEIT ...", 10);
                bg(move || timecfg::set_time(&t), |a, r| {
                    match r {
                        Ok(()) => a.message("ZEIT GESETZT, NTP AUS", 4),
                        Err(e) => a.message(&format!("FEHLER: {e}"), 6),
                    }
                    a.poll_time();
                });
            }
            ("net", "connect") => self.wifi_connect(arg, None),
            ("net", "disconnect") => self.wifi_disconnect(),
            ("net", "password") => self.input(&format!("pass:{arg}"), "WLAN-PASSWORT", &arg, true),
            ("net", "forget") => {
                self.wifi_busy = Some("LÖSCHE ...".into());
                self.refresh();
                bg(move || wifi::forget(&arg), |a, r| {
                    a.wifi_busy = None;
                    match r {
                        Ok(()) => a.message("PROFIL GELÖSCHT", 4),
                        Err(e) => a.message(&format!("FEHLER: {e}"), 6),
                    }
                    a.poll_wifi();
                    a.scan_wifi(false);
                });
            }
            ("dev", "yes") => self.bt_op("remove".to_string(), arg),
            ("dev", op) => self.bt_op(op.to_string(), arg),
            ("calib", "save") => {
                let Some(m) = self.calib_result.take() else { return };
                match touchcal::save(&m) {
                    Ok(()) => {
                        self.message("GESPEICHERT, KONSOLE STARTET NEU ...", 10);
                        // libinput reads the matrix only when the device is added
                        slint::Timer::single_shot(Duration::from_millis(1200), || {
                            let _ = slint::quit_event_loop();
                        });
                    }
                    Err(e) => self.message(&format!("FEHLER: {}", util::short_err(&e)), 8),
                }
            }
            ("calib", "again") => self.calib_start(),
            _ => {}
        }
    }

    /// Hand tty1 and the display to an emulator; its service stops the
    /// console (Conflicts=) and starts it again when the emulator exits.
    fn emu_start(&mut self, id: String) {
        let label = EMUS.iter().find(|e| e.0 == id).map(|e| e.1).unwrap_or("EMULATOR");
        self.message(&format!("STARTE {label} ..."), 30);
        let unit = format!("pocketchip-emu@{id}.service");
        bg(move || util::run("systemctl", &["start", "--no-block", &unit]), |a, (ok, _, err)| {
            if !ok {
                a.message(&format!("FEHLER: {}", util::short_err(&err)), 6);
            }
        });
    }

    fn input(&mut self, id: &str, title: &str, prompt: &str, secret: bool) {
        let be = self.be();
        be.set_input_id(id.into());
        be.set_input_title(title.into());
        be.set_input_prompt(prompt.into());
        be.set_input_secret(secret);
        be.set_input_open(true);
    }

    fn input_done(&mut self, id: String, text: String, ok: bool) {
        self.be().set_input_open(false);
        if !ok {
            return;
        }
        if let Some(ssid) = id.strip_prefix("pass:") {
            if text.is_empty() {
                self.message("KEIN PASSWORT EINGEGEBEN", 4);
                return;
            }
            self.wifi_connect(ssid.to_string(), Some(text));
        }
    }

    // ---- actions --------------------------------------------------------------

    fn activate(&mut self, id: String) {
        let page = self.page();
        match (&page, id.as_str()) {
            (Page::Home, "settings") => self.push(Page::Menu),
            (Page::Home, "emus") => self.push(Page::Emus),
            (Page::Overlay(_), "resume") => self.overlay_exit(0),
            (Page::Overlay(l), "quit_emu") => {
                let text = format!("{l} BEENDEN? NICHT GESPEICHERTER SPIELSTAND GEHT VERLOREN.");
                self.confirm("quit_emu", "BEENDEN", &text)
            }
            (Page::Overlay(_), "poweroff") => self.confirm("poweroff", "AUSSCHALTEN", "SYSTEM JETZT AUSSCHALTEN? NICHT GESPEICHERTER SPIELSTAND GEHT VERLOREN."),
            (Page::Overlay(_), "reboot") => self.confirm("reboot", "NEUSTART", "SYSTEM JETZT NEU STARTEN? NICHT GESPEICHERTER SPIELSTAND GEHT VERLOREN."),
            (Page::Overlay(_), "suspend") => self.do_power("suspend"),
            (Page::Overlay(_), _) => self.adjust(id, 1),
            (Page::Emus, e) if e.starts_with("emu:") => self.emu_start(e[4..].to_string()),
            (Page::Menu, "status") => self.push(Page::Status),
            (Page::Menu, "battery") => self.push(Page::Battery),
            (Page::Menu, "power") => self.push(Page::Power),
            (Page::Menu, "display") => self.push(Page::Display),
            (Page::Menu, "audio") => self.push(Page::Audio),
            (Page::Audio, "tone") => {
                if self.settings.muted || self.settings.volume == 0 {
                    self.message("STUMM GESCHALTET", 3);
                } else {
                    audio::test_tone();
                    self.message("TESTTON 440 HZ", 2);
                }
            }
            (Page::Audio, _) => self.adjust(id, 1),
            (Page::Menu, "wifi") => self.push(Page::Wifi),
            (Page::Menu, "bt") => self.push(Page::Bt),
            (Page::Menu, "time") => self.push(Page::Time),
            (Page::Power, "poweroff") => self.confirm("poweroff", "AUSSCHALTEN", "SYSTEM JETZT AUSSCHALTEN?"),
            (Page::Power, "reboot") => self.confirm("reboot", "NEUSTART", "SYSTEM JETZT NEU STARTEN?"),
            (Page::Power, "suspend") => self.confirm("suspend", "STANDBY", "IN STANDBY GEHEN? DISPLAY AUS, CPU GEDROSSELT. AUFWECKEN MIT TASTE, TOUCH ODER POWER-TASTE."),
            (Page::Display, "screen_now") => self.screen_set(false),
            (Page::Display, "touchcal") => self.calib_start(),
            (Page::Display, _) => self.adjust(id, 1),
            (Page::Wifi, "radio") => self.adjust(id, 1),
            (Page::Wifi, "disconnect") => self.wifi_disconnect(),
            (Page::Wifi, "scan") => self.scan_wifi(true),
            (Page::Wifi, n) if n.starts_with("net:") => {
                let Some(net) = n[4..].parse::<usize>().ok().and_then(|i| self.nets.get(i).cloned()) else { return };
                let open = net.security.is_empty() || net.security == "--";
                let id = format!("net:{}", net.ssid);
                if net.in_use {
                    self.dialog(&id, &net.ssid, "VERBUNDEN.", &[("TRENNEN", "disconnect"), ("VERGESSEN", "forget"), ("ABBRECHEN", "no")], 2);
                } else if net.known {
                    self.dialog(&id, &net.ssid, "GESPEICHERTES NETZ.", &[("VERBINDEN", "connect"), ("PASSWORT ÄNDERN", "password"), ("VERGESSEN", "forget"), ("ABBRECHEN", "no")], 0);
                } else if open {
                    self.dialog(&id, &net.ssid, "OFFENES NETZ OHNE VERSCHLÜSSELUNG. VERBINDEN?", &[("VERBINDEN", "connect"), ("ABBRECHEN", "no")], 1);
                } else {
                    self.input(&format!("pass:{}", net.ssid), "WLAN-PASSWORT", &net.ssid, true);
                }
            }
            (Page::Bt, "bt_service") => {
                self.bt_busy = Some("STARTE ...".into());
                self.refresh();
                bg(bt::start_service, |a, r| {
                    a.bt_busy = None;
                    if let Err(e) = r {
                        a.message(&format!("FEHLER: {e}"), 6);
                    }
                    a.poll_bt(true);
                });
            }
            (Page::Bt, "bt_power") => self.adjust(id, 1),
            (Page::Bt, "bt_scan") => {
                self.bt_busy = Some("SUCHE ...".into());
                self.refresh();
                bg(|| bt::scan(10), |a, r| {
                    a.bt_busy = None;
                    match r {
                        Ok(d) => {
                            a.message(&format!("{} GERÄTE", d.len()), 3);
                            a.bt_devs = d;
                        }
                        Err(e) => a.message(&format!("FEHLER: {e}"), 6),
                    }
                    a.refresh();
                });
            }
            (Page::Bt, n) if n.starts_with("dev:") => {
                let Some(d) = n[4..].parse::<usize>().ok().and_then(|i| self.bt_devs.get(i).cloned()) else { return };
                self.push(Page::BtDevice(d.mac));
            }
            (Page::BtDevice(mac), op) if op.starts_with("bd_") => {
                let mac = mac.clone();
                let name = self.bt_dev.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| mac.clone());
                match op {
                    "bd_remove" => self.confirm(&format!("dev:{mac}"), "ENTFERNEN", &format!("{name} ENTFERNEN? DIE KOPPLUNG WIRD GELÖSCHT.")),
                    _ => self.bt_op(op[3..].to_string(), mac),
                }
            }
            (Page::Time, "ntp") => self.adjust(id, 1),
            (Page::Time, "tz") => self.push(Page::TzRegion),
            (Page::Time, "settime") => {
                let e = self.edit;
                let text = format!(
                    "ZEIT AUF {:02}.{:02}.{:04} {:02}:{:02} SETZEN?{}",
                    e.day,
                    e.month,
                    e.year,
                    e.hour,
                    e.minute,
                    if self.time.ntp { " NETZWERKZEIT WIRD ABGESCHALTET." } else { "" }
                );
                self.confirm("settime", "ZEIT SETZEN", &text);
            }
            (Page::TzRegion, r) if r.starts_with("r:") => {
                let region = r[2..].to_string();
                if self.zones.iter().any(|z| z.starts_with(&format!("{region}/"))) {
                    self.push(Page::TzCity(region));
                } else {
                    self.set_zone(region);
                }
            }
            (Page::TzCity(_), z) if z.starts_with("z:") => self.set_zone(z[2..].to_string()),
            _ => {}
        }
    }

    fn set_zone(&mut self, z: String) {
        self.message("SETZE ZEITZONE ...", 10);
        bg(move || timecfg::set_zone(&z).map(|_| z), |a, r| {
            match r {
                Ok(z) => {
                    timecfg::apply_tz(&z);
                    a.message(&format!("ZEITZONE {}", z.to_uppercase()), 4);
                    a.time.timezone = z;
                    while a.stack.len() > 1 && a.page() != Page::Time {
                        a.stack.pop();
                        a.restore_sel = true;
                    }
                }
                Err(e) => a.message(&format!("FEHLER: {e}"), 6),
            }
            a.refresh();
        });
    }

    fn adjust(&mut self, id: String, d: i32) {
        let page = match self.page() {
            Page::Overlay(_) if id == "bright" => Page::Display,
            Page::Overlay(_) if id == "vol" || id == "mute" => Page::Audio,
            p => p,
        };
        match (&page, id.as_str()) {
            (Page::Display, "bright") => {
                self.settings.brightness = (self.settings.brightness + d as i64).clamp(1, self.backlight.max);
                self.backlight.set(self.settings.brightness);
                self.settings.save();
            }
            (Page::Display, "screen") => {
                self.settings.screen_timeout = step(SCREEN_STEPS, self.settings.screen_timeout, d);
                self.settings.save();
            }
            (Page::Display, "standby") => {
                self.settings.standby_timeout = step(STANDBY_STEPS, self.settings.standby_timeout, d);
                self.settings.save();
            }
            (Page::Display, "standby_bat") => {
                self.settings.standby_on_battery_only = !self.settings.standby_on_battery_only;
                self.settings.save();
            }
            (Page::Audio, "vol") => {
                self.settings.volume = (self.settings.volume + 5 * d as i64).clamp(0, 100);
                let (v, m) = (self.settings.volume, self.settings.muted);
                std::thread::spawn(move || audio::apply(v, m));
                self.settings.save();
            }
            (Page::Audio, "mute") => {
                self.settings.muted = !self.settings.muted;
                let (v, m) = (self.settings.volume, self.settings.muted);
                std::thread::spawn(move || audio::apply(v, m));
                self.settings.save();
            }
            (Page::Display, "led_on") => {
                self.settings.led_on = !self.settings.led_on;
                self.apply_led();
                self.settings.save();
            }
            (Page::Display, "led_standby") => {
                self.settings.led_standby_blink = !self.settings.led_standby_blink;
                self.settings.save();
            }
            (Page::Display, "scanlines") => {
                self.settings.scanlines = !self.settings.scanlines;
                self.be().set_scanlines(self.settings.scanlines);
                self.settings.save();
            }
            (Page::Wifi, "radio") => {
                let on = !self.wifi.radio;
                self.wifi_busy = Some(if on { "SCHALTE EIN ..." } else { "SCHALTE AUS ..." }.into());
                self.refresh();
                bg(move || wifi::set_radio(on), move |a, r| {
                    a.wifi_busy = None;
                    if let Err(e) = r {
                        a.message(&format!("FEHLER: {e}"), 6);
                    }
                    if !on {
                        a.nets.clear();
                    }
                    a.poll_wifi();
                    if on {
                        // NetworkManager needs a moment before scanning works
                        slint::Timer::single_shot(Duration::from_secs(3), || with_app(|a| a.scan_wifi(true)));
                    }
                });
            }
            (Page::Bt, "bt_power") => {
                if self.bt_busy.is_some() {
                    return;
                }
                let on = !(self.bt.powered && !self.bt.blocked);
                self.bt_busy = Some(if on { "SCHALTE EIN ..." } else { "SCHALTE AUS ..." }.into());
                self.refresh();
                bg(move || bt::set_power(on), |a, r| {
                    a.bt_busy = None;
                    if let Err(e) = r {
                        a.message(&format!("FEHLER: {e}"), 6);
                    }
                    a.poll_bt(true);
                });
            }
            (Page::Time, "ntp") => {
                let on = !self.time.ntp;
                bg(move || timecfg::set_ntp(on), |a, r| {
                    if let Err(e) = r {
                        a.message(&format!("FEHLER: {e}"), 6);
                    }
                    a.poll_time();
                });
            }
            (Page::Time, f) => {
                let e = &mut self.edit;
                match f {
                    "y" => e.year = (e.year + d).clamp(2024, 2099),
                    "mo" => e.month = (e.month - 1 + d).rem_euclid(12) + 1,
                    "d" => {
                        let n = timecfg::days_in_month(e.year, e.month);
                        e.day = (e.day - 1 + d).rem_euclid(n) + 1
                    }
                    "h" => e.hour = (e.hour + d).rem_euclid(24),
                    "mi" => e.minute = (e.minute + d).rem_euclid(60),
                    _ => {}
                }
                let n = timecfg::days_in_month(e.year, e.month);
                e.day = e.day.min(n);
            }
            _ => {}
        }
        self.refresh();
    }

    // ---- wifi / bt / time polling --------------------------------------------

    fn poll_wifi(&mut self) {
        bg(
            || (wifi::state(), sys::ip_addrs()),
            |a, (w, ips)| {
                a.wifi = w;
                a.ips = ips;
                a.refresh();
            },
        );
    }

    fn scan_wifi(&mut self, rescan: bool) {
        if self.wifi_busy.is_some() {
            return;
        }
        self.wifi_busy = Some("SUCHE ...".into());
        self.refresh();
        bg(move || wifi::scan(rescan), move |a, r| {
            a.wifi_busy = None;
            match r {
                Ok(n) => {
                    if rescan {
                        a.message(&format!("{} NETZE GEFUNDEN", n.len()), 3);
                    }
                    a.nets = n;
                }
                Err(e) => a.message(&format!("FEHLER: {e}"), 6),
            }
            a.refresh();
            // first a fast cached list, then a real rescan
            if !rescan && a.page() == Page::Wifi && a.wifi.radio {
                a.scan_wifi(true);
            }
        });
    }

    fn wifi_connect(&mut self, ssid: String, pass: Option<String>) {
        self.wifi_busy = Some("VERBINDE ...".into());
        self.message(&format!("VERBINDE MIT {} ...", ssid.to_uppercase()), 45);
        self.refresh();
        let ssid2 = ssid.clone();
        bg(move || wifi::connect(&ssid, pass.as_deref()).map(|_| ssid), move |a, r| {
            a.wifi_busy = None;
            match r {
                Ok(s) => a.message(&format!("VERBUNDEN: {}", s.to_uppercase()), 4),
                Err(e) => {
                    a.message(&format!("FEHLER: {e}"), 8);
                    a.dialog(
                        &format!("net:{ssid2}"),
                        &ssid2,
                        &format!("VERBINDUNG FEHLGESCHLAGEN: {e}"),
                        &[("PASSWORT EINGEBEN", "password"), ("NOCHMAL", "connect"), ("ABBRECHEN", "no")],
                        0,
                    );
                }
            }
            a.poll_wifi();
            a.scan_wifi(false);
        });
    }

    fn wifi_disconnect(&mut self) {
        let Some(dev) = self.wifi.device.clone() else { return };
        self.wifi_busy = Some("TRENNE ...".into());
        self.refresh();
        bg(move || wifi::disconnect(&dev), |a, r| {
            a.wifi_busy = None;
            match r {
                Ok(()) => a.message("GETRENNT", 3),
                Err(e) => a.message(&format!("FEHLER: {e}"), 6),
            }
            a.poll_wifi();
            a.scan_wifi(false);
        });
    }

    fn poll_bt(&mut self, with_devices: bool) {
        bg(
            move || {
                let st = bt::state();
                let devs = if with_devices && st.adapter { Some(bt::devices()) } else { None };
                (st, devs)
            },
            |a, (st, devs)| {
                a.bt = st;
                if let Some(d) = devs {
                    a.bt_devs = d;
                }
                a.refresh();
            },
        );
    }

    fn poll_bt_dev(&mut self, mac: String) {
        bg(move || bt::info(&mac), |a, d| {
            if a.page() == Page::BtDevice(d.mac.clone()) {
                a.bt_dev = Some(d);
                a.refresh();
            }
        });
    }

    fn bt_op(&mut self, op: String, mac: String) {
        self.bt_busy = Some(match op.as_str() {
            "pair" => "KOPPLE ...",
            "connect" => "VERBINDE ...",
            "disconnect" => "TRENNE ...",
            _ => "ENTFERNE ...",
        }
        .into());
        self.refresh();
        bg(
            move || match op.as_str() {
                "pair" => bt::pair(&mac),
                "connect" => bt::connect(&mac),
                "disconnect" => bt::disconnect(&mac),
                "remove" => bt::remove(&mac),
                _ => Ok(()),
            },
            move |a, r| {
                a.bt_busy = None;
                match r {
                    Ok(()) => a.message("OK", 3),
                    Err(e) => a.message(&format!("FEHLER: {e}"), 8),
                }
                a.poll_bt(true);
                if let Page::BtDevice(m) = a.page() {
                    a.poll_bt_dev(m);
                }
            },
        );
    }

    fn poll_time(&mut self) {
        bg(timecfg::state, |a, t| {
            if t.timezone != a.time.timezone {
                timecfg::apply_tz(&t.timezone);
            }
            a.time = t;
            a.refresh();
        });
    }

    // ---- power and screen -----------------------------------------------------

    fn do_power(&mut self, what: &str) {
        match what {
            "poweroff" => {
                self.message("SYSTEM WIRD AUSGESCHALTET ...", 60);
                bg(|| util::run("systemctl", &["poweroff"]), |a, (ok, _, e)| {
                    if !ok {
                        a.message(&format!("FEHLER: {}", util::short_err(&e)), 6)
                    }
                });
            }
            "reboot" => {
                self.message("NEUSTART ...", 60);
                bg(|| util::run("systemctl", &["reboot"]), |a, (ok, _, e)| {
                    if !ok {
                        a.message(&format!("FEHLER: {}", util::short_err(&e)), 6)
                    }
                });
            }
            "suspend" => self.suspend(),
            _ => {}
        }
    }

    /// Light standby: kernel s2idle does not resume on this board, so the
    /// console only blanks the display and caps the CPU clock. Any key, touch
    /// or the power key wakes it at once.
    fn suspend(&mut self) {
        if self.standby {
            return;
        }
        self.standby = true;
        self.be().set_dialog_open(false);
        self.be().set_input_open(false);
        self.screen_set(false);
        sys::cpu_cap(true);
        self.apply_led();
        // ignore the key press that confirmed the dialog
        self.suspending_until = mono_ms() + 800;
    }

    fn apply_led(&self) {
        let mode = if self.standby {
            if self.settings.led_standby_blink { led::Mode::Blink } else { led::Mode::Off }
        } else if self.settings.led_on {
            led::Mode::Normal
        } else {
            led::Mode::Off
        };
        led::apply(mode);
    }

    fn wake_standby(&mut self) {
        if !self.standby {
            return;
        }
        self.standby = false;
        sys::cpu_cap(false);
        self.apply_led();
        hw::LAST_ACTIVITY.store(mono_ms(), Ordering::Relaxed);
        self.screen_set(true);
        self.message("ZURÜCK AUS STANDBY", 3);
        self.poll_wifi();
    }

    /// Called after the system came back from a real suspend.
    fn resumed(&mut self) {
        let now = mono_ms();
        self.suspending_until = 0;
        self.ignore_power_until = now + 2500;
        hw::LAST_ACTIVITY.store(now, Ordering::Relaxed);
        self.screen_set(true);
        self.message("ZURÜCK AUS STANDBY", 3);
        self.poll_wifi();
    }

    fn check_resume(&mut self) -> bool {
        let s = util::suspended_ms();
        if s > self.last_suspended + 1000 {
            self.last_suspended = s;
            self.resumed();
            return true;
        }
        self.last_suspended = self.last_suspended.max(s);
        false
    }

    fn screen_set(&mut self, on: bool) {
        if on == !self.screen_off {
            return;
        }
        self.screen_off = !on;
        if on {
            self.backlight.power(true);
            self.backlight.set(self.settings.brightness);
            hw::LAST_ACTIVITY.store(mono_ms(), Ordering::Relaxed);
            // keep the black curtain a moment so the waking tap is swallowed
            slint::Timer::single_shot(Duration::from_millis(450), || {
                with_app(|a| {
                    if !a.screen_off {
                        a.be().set_screen_off(false);
                    }
                })
            });
        } else {
            self.be().set_screen_off(true);
            self.backlight.power(false);
        }
    }

    // ---- touch calibration ----------------------------------------------------

    fn calib_start(&mut self) {
        self.calib.clear();
        self.calib_result = None;
        hw::CALIBRATING.store(true, Ordering::Relaxed);
        self.be().set_calib_open(true);
        self.calib_show();
    }

    fn calib_show(&mut self) {
        let i = self.calib.len();
        let (x, y) = touchcal::TARGETS[i];
        let be = self.be();
        be.set_calib_x(x as f32);
        be.set_calib_y(y as f32);
        be.set_calib_text(format!("PUNKT {} VON {}", i + 1, touchcal::TARGETS.len()).into());
    }

    fn calib_cancel(&mut self) {
        hw::CALIBRATING.store(false, Ordering::Relaxed);
        self.be().set_calib_open(false);
        self.message("KALIBRIERUNG ABGEBROCHEN", 3);
    }

    fn calib_sample(&mut self, x: f64, y: f64) {
        if !self.be().get_calib_open() {
            return;
        }
        self.calib.push((x, y));
        if self.calib.len() < touchcal::TARGETS.len() {
            self.calib_show();
            return;
        }
        hw::CALIBRATING.store(false, Ordering::Relaxed);
        self.be().set_calib_open(false);
        match touchcal::compute(&self.calib) {
            Some(r) => {
                self.calib_result = Some(r.matrix);
                let warn = if r.max_px > 12.0 { " UNGENAU, BESSER WIEDERHOLEN." } else { "" };
                let text = format!(
                    "ABWEICHUNG IM MITTEL {:.1} PX, MAX {:.1} PX.{warn} SPEICHERN? DIE KONSOLE STARTET DANACH NEU.",
                    r.mean_px, r.max_px
                );
                self.dialog("calib", "KALIBRIERUNG", &text, &[("SPEICHERN", "save"), ("WIEDERHOLEN", "again"), ("ABBRECHEN", "no")], if r.max_px > 12.0 { 1 } else { 0 });
            }
            None => {
                self.message("MESSUNG UNBRAUCHBAR, BITTE WIEDERHOLEN", 6);
            }
        }
    }

    fn on_input(&mut self, ev: hw::InputEvent) {
        let now = mono_ms();
        if self.check_resume() {
            return;
        }
        match ev {
            hw::InputEvent::TouchRaw(x, y) => self.calib_sample(x, y),
            hw::InputEvent::Activity => {
                if now <= self.suspending_until {
                    return;
                }
                if self.standby {
                    self.wake_standby();
                } else if self.screen_off {
                    self.screen_set(true);
                }
            }
            hw::InputEvent::PowerKey => {
                if now < self.ignore_power_until || now < self.suspending_until {
                    return;
                }
                hw::LAST_ACTIVITY.store(now, Ordering::Relaxed);
                if self.standby {
                    self.wake_standby();
                    return;
                }
                if self.screen_off {
                    self.screen_set(true);
                    return;
                }
                if self.be().get_input_open() {
                    self.be().set_input_open(false);
                }
                if self.overlay().is_some() {
                    // the power key opened the pause menu; pressed again it resumes
                    if self.be().get_dialog_open() {
                        self.be().set_dialog_open(false);
                    }
                    self.overlay_exit(0);
                    return;
                }
                if self.be().get_dialog_open() && self.be().get_dialog_id() == "power" {
                    return;
                }
                self.power_menu();
            }
        }
    }

    fn tick(&mut self) {
        self.tick_n += 1;
        let now = mono_ms();
        self.check_resume();

        let t = timecfg::now();
        self.be().set_clock(format!("{:02}:{:02}", t.hour, t.minute).into());
        if self.tick_n % 2 == 1 {
            self.cpu.sample();
        }
        self.bat = sys::Battery::read();
        self.be().set_battery(self.bat.short().into());
        self.be().set_net(
            if !self.wifi.radio {
                "WLAN AUS"
            } else if self.wifi.ssid.is_some() {
                "WLAN ▮"
            } else {
                "WLAN –"
            }
            .into(),
        );
        if self.msg_until > 0 && now > self.msg_until {
            self.msg_until = 0;
            self.be().set_message("".into());
        }
        if self.tick_n % 15 == 0 {
            self.poll_wifi();
        }
        if self.tick_n % 5 == 0 && self.page() == Page::Time {
            self.poll_time();
        }
        if self.tick_n % 10 == 0 && self.page() != Page::Bt && self.bt_busy.is_none() {
            self.poll_bt(false);
        }
        if self.tick_n % 5 == 0 && self.page() == Page::Bt && self.bt_busy.is_none() {
            self.poll_bt(true);
        }
        if self.tick_n % 3 == 0 && self.bt_busy.is_none() {
            if let Page::BtDevice(m) = self.page() {
                self.poll_bt_dev(m);
            }
        }
        if self.tick_n % 10 == 0 && self.page() == Page::Status {
            self.ips = sys::ip_addrs();
        }

        // idle handling
        let idle = now.saturating_sub(hw::LAST_ACTIVITY.load(Ordering::Relaxed)) / 1000;
        let st = self.settings.screen_timeout as u64;
        if !self.screen_off && st > 0 && idle >= st && !self.be().get_input_open() {
            self.screen_set(false);
        }
        let sb = self.settings.standby_timeout as u64;
        let allowed = !(self.settings.standby_on_battery_only && self.bat.external_power());
        if sb > 0 && idle >= sb && allowed && !self.standby && now > self.suspending_until {
            self.suspend();
        }

        // pages with live values
        match self.page() {
            Page::Menu | Page::Status | Page::Battery | Page::Time | Page::Overlay(_) => {
                if !self.be().get_dialog_open() {
                    self.refresh()
                }
            }
            _ => {}
        }
    }
}

fn main() {
    hw::install_signal_quit();
    // pause menu over an emulator: started by pocketchip-emu-helper
    let args: Vec<String> = std::env::args().collect();
    let overlay = args.iter().position(|a| a == "--overlay").map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "EMULATOR".into()).to_uppercase());

    #[cfg(feature = "kms")]
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "linuxkms-software");
    }

    let ui = AppWindow::new().expect("UI init failed");
    let _vt = hw::VtGuard::take();

    let settings = Settings::load();
    let backlight = sys::Backlight::open();
    backlight.power(true);
    backlight.set(settings.brightness);
    hw::LAST_ACTIVITY.store(mono_ms(), Ordering::Relaxed);

    let rows = Rc::new(VecModel::<Row>::default());
    let be = ui.global::<Backend>();
    be.set_rows(ModelRc::from(rows.clone()));
    be.set_scanlines(settings.scanlines);

    be.on_activate(|id| with_app(move |a| a.activate(id.to_string())));
    be.on_adjust(|id, d| with_app(move |a| a.adjust(id.to_string(), d)));
    be.on_back(|| with_app(|a| a.back()));
    be.on_selected(|_| {});
    be.on_dialog_result(|id, i| with_app(move |a| a.dialog_result(id.to_string(), i)));
    be.on_input_done(|id, text, ok| with_app(move |a| a.input_done(id.to_string(), text.to_string(), ok)));
    be.on_calib_cancel(|| with_app(|a| a.calib_cancel()));
    be.on_key_activity(|| hw::LAST_ACTIVITY.store(mono_ms(), Ordering::Relaxed));

    let app = App {
        ui: ui.clone_strong(),
        rows,
        stack: vec![(overlay.clone().map(Page::Overlay).unwrap_or(Page::Home), 0)],
        settings,
        backlight,
        cpu: sys::CpuSampler::default(),
        bat: sys::Battery::read(),
        ips: Vec::new(),
        wifi: wifi::WifiState::default(),
        nets: Vec::new(),
        wifi_busy: None,
        bt: bt::BtState::default(),
        bt_devs: Vec::new(),
        bt_busy: None,
        bt_dev: None,
        time: timecfg::TimeState::default(),
        edit: timecfg::now(),
        zones: Vec::new(),
        dialog_actions: Vec::new(),
        screen_off: false,
        suspending_until: 0,
        ignore_power_until: 0,
        last_suspended: util::suspended_ms(),
        msg_until: 0,
        tick_n: 0,
        restore_sel: true,
        calib: Vec::new(),
        calib_result: None,
        standby: false,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    with_app(|a| {
        a.apply_led();
        let (v, m) = (a.settings.volume, a.settings.muted);
        std::thread::spawn(move || audio::apply(v, m));
        a.cpu.sample();
        a.refresh();
        a.tick();
        a.poll_wifi();
        a.poll_bt(false);
        a.poll_time();
        if a.overlay().is_some() {
            a.ips = sys::ip_addrs();
            // ignore the release of the power key that opened the menu
            a.ignore_power_until = mono_ms() + 600;
        }
    });

    hw::spawn_input_watcher(|ev| {
        let _ = slint::invoke_from_event_loop(move || with_app(move |a| a.on_input(ev)));
    });

    let _ = std::fs::create_dir_all(debug::DIR);
    unsafe { libc::chmod(c"/run/pocketchip-console".as_ptr(), 0o700) };
    let dbg_ui = ui.as_weak();
    let dbg_timer = slint::Timer::default();
    dbg_timer.start(slint::TimerMode::Repeated, Duration::from_millis(300), move || {
        if let Some(ui) = dbg_ui.upgrade() {
            debug::poll(&ui);
        }
    });

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), || with_app(|a| a.tick()));

    ui.run().expect("event loop failed");
    sys::cpu_cap(false);
    // leave the LEDs in their normal state when the console is not running
    led::apply(led::Mode::Normal);
    APP.with(|a| *a.borrow_mut() = None);
    if overlay.is_some() {
        std::process::exit(EXIT_CODE.load(Ordering::Relaxed));
    }
}
