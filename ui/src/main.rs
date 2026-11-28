mod audio;
mod bt;
mod btscan;
mod debug;
mod hw;
mod led;
mod radar;
mod settings;
mod sys;
mod term;
mod timecfg;
mod touchcal;
mod update;
mod upload;
mod util;
mod wifi;

use settings::{fmt_percent_or_never, fmt_timeout, step, Settings, BATTERY_OFF_STEPS, BATTERY_WARN_STEPS, SCREEN_STEPS, STANDBY_STEPS};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
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
    Term,
    Help,
    Update,
    Upload,
    /// Pause menu over a running emulator (`--overlay LABEL`).
    Overlay(String),
    TzRegion,
    TzCity(String),
    BtDevice(String),
    Radar,
    /// radar detail page, by D-Bus object path
    RadarDev(String),
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
            Page::Term => "TERMINAL".into(),
            Page::Help => "HILFE".into(),
            Page::Update => "UPDATE".into(),
            Page::Upload => "UPLOAD".into(),
            Page::Overlay(l) => format!("PAUSE / {l}"),
            Page::TzRegion => "ZEITZONE".into(),
            Page::TzCity(r) => format!("ZEITZONE / {}", r.to_uppercase()),
            Page::BtDevice(_) => "BLUETOOTH / GERÄT".into(),
            Page::Radar => "BT-RADAR".into(),
            Page::RadarDev(_) => "BT-RADAR / GERÄT".into(),
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

/// Keyboard and touch reference shown on the HILFE page.
fn help_rows() -> Vec<Row> {
    let mut v = vec![sep("KONSOLE")];
    for (k, d) in [
        ("↑ ↓", "AUSWAHL"),
        ("◀ ▶", "WERT / KACHEL"),
        ("ENTER", "ÖFFNEN / UMSCHALTEN"),
        ("ESC", "ZURÜCK"),
        ("STATUSZEILE TIPPEN", "ZURÜCK"),
        ("POWER-TASTE", "AUSSCHALT-DIALOG"),
        ("POWER 6 S HALTEN", "HARTES AUS"),
    ] {
        v.push(info(k, d));
    }
    v.push(sep("FN-TASTE + ..."));
    for (k, d) in [
        ("FN + Y / U", "{  }"),
        ("FN + I / O", "[  ]"),
        ("FN + P", "|"),
        ("FN + H / J", "<  >"),
        ("FN + K / L", "'  \""),
        ("FN + B", "`"),
        ("FN + N", "~"),
        ("FN + M", ":"),
        ("FN + .", ";"),
        ("FN + /", "\\"),
        ("FN + ⌫", "ENTF"),
        ("FN + ▲ / ▼", "BILD AUF / AB"),
        ("FN + ◀ / ▶", "POS1 / ENDE"),
    ] {
        v.push(info(k, d));
    }
    v.push(sep("SHIFT + ..."));
    for (k, d) in [
        ("SHIFT + 1 ... 0", "! @ # $ % ^ & * ( )"),
        ("SHIFT + .", ","),
        ("SHIFT + +", "="),
        ("SHIFT + -", "_"),
        ("SHIFT + /", "?"),
    ] {
        v.push(info(k, d));
    }
    v.push(sep("BT-RADAR"));
    for (k, d) in [
        ("ENTFERNUNG", "GESCHÄTZT AUS SIGNALSTÄRKE"),
        ("RICHTUNG", "NICHT MESSBAR"),
        ("WINKEL DER PUNKTE", "ERFUNDEN, FEST JE GERÄT"),
        ("NAH / MITTEL / FERN", "> -60 / -75 / -88 DBM"),
        ("PUNKT ANTIPPEN", "DETAILS"),
        ("AUSGEBLENDET", "NACH 30-60 S OHNE SIGNAL"),
    ] {
        v.push(info(k, d));
    }
    v.push(sep("TERMINAL"));
    for (k, d) in [
        ("LEISTE UNTEN", "ESC TAB CTRL ▲ ▼"),
        ("CTRL (LEISTE)", "FÜR NÄCHSTE TASTE"),
        ("SHIFT+FN+▲ / ▼", "VERLAUF BLÄTTERN"),
        ("FINGER ZIEHEN", "VERLAUF BLÄTTERN"),
        ("CTRL + C", "ABBRECHEN"),
        ("CTRL + D", "SHELL BEENDEN"),
        ("CTRL + P", "PROMPT 1 / 2 ZEILEN"),
        ("◀ ZURÜCK", "MENÜ, SHELL LÄUFT"),
        ("SCHRIFT", "ANZEIGE › TERMINAL"),
    ] {
        v.push(info(k, d));
    }
    v
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
    /// low battery warning already shown for this discharge
    bat_warned: bool,
    /// seconds left before the low battery power-off (None = not armed)
    bat_off_in: Option<u32>,
    /// debug: pretend this battery state (Some(level), -1 = on charger)
    pub bat_fake: Option<i64>,
    term: Option<term::Term>,
    term_segs: Rc<VecModel<TermSeg>>,
    term_ctrl: bool,
    term_redraw_pending: bool,
    upd_pkgs: Option<Vec<(String, String)>>,
    upd_checking: bool,
    upd_last_check: String,
    upd_progress: update::Progress,
    upd_total: usize,
    upd_poll_busy: bool,
    upload_on: bool,
    upload_pin: String,
    upload_busy: bool,
    radar_scan: Option<btscan::Scanner>,
    radar_started: u64,
    radar_renderer: Option<radar::Renderer>,
    radar_timer: slint::Timer,
    radar_blips: Vec<radar::Blip>,
    radar_devs: Vec<(btscan::Dev, i16)>,
    radar_sel: Option<String>,
    radar_items: Rc<VecModel<RadarItem>>,
    radar_tiles: Rc<VecModel<slint::Image>>,
    radar_prev_trail: u64,
    radar_frame_n: u64,
    radar_static_at: u64,
    /// repaint all tiles with the next frame
    radar_full: bool,
    oui_cache: HashMap<String, Option<String>>,
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
            Page::Term => self.term_open(),
            Page::Upload => {
                self.ips = sys::ip_addrs();
                self.upload_poll();
            }
            Page::Update => self.update_poll(),
            Page::Radar => {
                self.radar_full = true;
                self.radar_sync();
                self.radar_update();
                self.radar_frame();
            }
            Page::RadarDev(key) => self.radar_oui(key.clone()),
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
        be.set_term(page == Page::Term);
        be.set_plain(page == Page::Help || page == Page::Radar);
        be.set_radar(page == Page::Radar);
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
        self.radar_sync();
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
            Page::Home => vec![
                action("settings", "EINSTELLUNGEN", ""),
                action("term", "TERMINAL", ""),
                action("emus", "EMULATOREN", ""),
                action("upload", "UPLOAD", ""),
                action("help", "HILFE", ""),
                action("radar", "BT-RADAR", ""),
            ],
            Page::Radar => Vec::new(),
            Page::RadarDev(key) => self.rows_radar_dev(key),
            Page::Term => Vec::new(),
            Page::Help => help_rows(),
            Page::Update => self.rows_update(),
            Page::Upload => self.rows_upload(),
            Page::Emus => self.rows_emus(),
            Page::Overlay(l) => self.rows_overlay(l),
            Page::Menu => self.rows_menu(),
            Page::Status => self.rows_status(),
            Page::Battery => self.rows_battery(),
            Page::Power => vec![
                action("poweroff", "AUSSCHALTEN", ""),
                action("reboot", "NEUSTART", ""),
                action("suspend", "STANDBY", ""),
                sep("AKKU"),
                adjust("bat_warn", "WARNUNG BEI", &fmt_percent_or_never(self.settings.battery_warn)),
                adjust("bat_off", "AUSSCHALTEN BEI", &fmt_percent_or_never(self.settings.battery_off)),
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
                    row("term_big", "TERMINAL-SCHRIFT", if s.term_big { "GROSS" } else { "KLEIN" }, 1, true),
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
            action(
                "update",
                "09 UPDATE",
                &if self.upd_progress.running {
                    "LÄUFT ...".to_string()
                } else {
                    match &self.upd_pkgs {
                        Some(p) if !p.is_empty() => format!("{} UPDATES", p.len()),
                        Some(_) => "AKTUELL".to_string(),
                        None => String::new(),
                    }
                },
            ),
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
            ("lowbat_off", "now") => self.do_power("poweroff"),
            ("upd", "yes") => self.update_start(),
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
            (Page::Home, "term") => self.push(Page::Term),
            (Page::Home, "help") => self.push(Page::Help),
            (Page::Home, "upload") => self.push(Page::Upload),
            (Page::Home, "radar") => self.push(Page::Radar),
            (Page::RadarDev(_), "bt_settings") | (Page::Radar, "bt_settings") => self.push(Page::Bt),
            (Page::Menu, "update") => self.push(Page::Update),
            (Page::Update, "check") => self.update_check(),
            (Page::Update, "upgrade") => {
                let b = &self.bat;
                if b.present && !b.external_power() && b.capacity >= 0 && b.capacity < 30 {
                    self.message("AKKU UNTER 30 %: BITTE LADEGERÄT ANSCHLIESSEN", 6);
                } else {
                    let n = self.upd_pkgs.as_ref().map(|p| p.len()).unwrap_or(0);
                    let text = format!("JETZT {n} PAKETE AKTUALISIEREN? DAS DAUERT EINIGE MINUTEN. GERÄT DABEI NICHT AUSSCHALTEN.");
                    self.confirm("upd", "UPDATE", &text);
                }
            }
            (Page::Update, "reboot") => self.confirm("reboot", "NEUSTART", "SYSTEM JETZT NEU STARTEN?"),
            (Page::Upload, "upl") => self.adjust(id, 1),
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
            (Page::Power, "bat_warn") | (Page::Power, "bat_off") => self.adjust(id, 1),
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
            (Page::Power, "bat_warn") => {
                let v = step(BATTERY_WARN_STEPS, self.settings.battery_warn as u32, d) as i64;
                // the warning must come before the power-off
                if v == 0 || v > self.settings.battery_off {
                    self.settings.battery_warn = v;
                    self.settings.save();
                }
            }
            (Page::Power, "bat_off") => {
                let v = step(BATTERY_OFF_STEPS, self.settings.battery_off as u32, d) as i64;
                if self.settings.battery_warn == 0 || v < self.settings.battery_warn {
                    self.settings.battery_off = v;
                    self.settings.save();
                }
            }
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
            (Page::Upload, "upl") => {
                if self.upload_busy {
                    return;
                }
                self.upload_busy = true;
                let on = !self.upload_on;
                let pin = upload::new_pin();
                let pin2 = pin.clone();
                bg(
                    move || if on { upload::start(&pin2) } else { upload::stop() },
                    move |a, r| {
                        a.upload_busy = false;
                        match r {
                            Ok(()) => {
                                a.upload_on = on;
                                a.upload_pin = if on { pin } else { String::new() };
                                a.message(if on { "WEBSERVER LÄUFT" } else { "WEBSERVER AUS" }, 3);
                            }
                            Err(e) => a.message(&format!("FEHLER: {e}"), 6),
                        }
                        a.ips = sys::ip_addrs();
                        a.refresh();
                    },
                );
                return;
            }
            (Page::Display, "term_big") => {
                self.settings.term_big = !self.settings.term_big;
                self.settings.save();
                self.term_apply_geom();
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
        self.radar_sync();
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

    // ---- update --------------------------------------------------------------

    // ---- Bluetooth radar -------------------------------------------------------

    fn radar_wanted(&self) -> bool {
        matches!(self.page(), Page::Radar | Page::RadarDev(_)) && !self.screen_off && !self.standby
    }

    /// Start/stop the LE scan and the beam animation to match the page and
    /// the display state. Never changes the Bluetooth power state.
    fn radar_sync(&mut self) {
        let now = mono_ms();
        if self.radar_wanted() {
            let retry = self.radar_scan.as_ref().map(|s| s.finished() && now > self.radar_started + 5000).unwrap_or(true);
            if retry {
                self.radar_scan = Some(btscan::Scanner::start());
                self.radar_started = now;
            }
        } else if let Some(s) = self.radar_scan.take() {
            s.stop();
        }
        let animate = self.radar_wanted() && self.page() == Page::Radar;
        if animate && !self.radar_timer.running() {
            self.radar_timer.start(slint::TimerMode::Repeated, Duration::from_millis(radar::FRAME_MS), || with_app(|a| a.radar_frame()));
        } else if !animate && self.radar_timer.running() {
            self.radar_timer.stop();
        }
    }

    /// Pull the devices from the scanner (once a second).
    fn radar_update(&mut self) {
        let now = mono_ms();
        let Some(scan) = self.radar_scan.as_ref() else { return };
        let status = scan.status();
        let mut devs: Vec<(btscan::Dev, i16)> = scan
            .snapshot()
            .into_iter()
            .filter(|d| !d.gone && now.saturating_sub(d.last_seen) <= radar::STALE_MS)
            .filter_map(|d| {
                let r = d.rssi_smooth(now, radar::SMOOTH_MS)?;
                Some((d, r))
            })
            .collect();
        if status != btscan::Status::Scanning {
            devs.clear();
        }
        devs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.addr.cmp(&b.0.addr)));
        self.radar_blips = devs
            .iter()
            .map(|(d, r)| {
                let angle = radar::angle_of(&d.addr);
                let (x, y) = radar::blip_pos(angle, radar::radius_frac(*r));
                radar::Blip { key: d.path.clone(), x, y, angle, first: d.first_seen, fresh: now.saturating_sub(d.last_seen) < radar::FRESH_MS }
            })
            .collect();
        let items: Vec<RadarItem> = devs
            .iter()
            .map(|(d, r)| {
                let mut info = radar::zone(*r).to_string();
                if let Some(m) = radar::distance_m(*r, d.tx_power) {
                    info += &format!(" · {}", radar::fmt_distance(m));
                }
                if !d.name.is_empty() {
                    if let Some(t) = radar::dev_type(d) {
                        info += &format!(" · {t}");
                    } else if let Some(m) = d.manuf.first() {
                        info += &format!(" · {}", radar::company_short(m.0).to_uppercase());
                    }
                } else if d.addr.len() >= 8 && radar::label(d) != d.addr {
                    // tell nameless devices apart
                    info += &format!(" · …{}", &d.addr[d.addr.len() - 5..]);
                }
                RadarItem {
                    id: d.path.clone().into(),
                    name: radar::label(d).into(),
                    rssi: r.to_string().into(),
                    info: info.into(),
                    fresh: now.saturating_sub(d.last_seen) < radar::FRESH_MS,
                }
            })
            .collect();
        let ui = self.ui.clone_strong();
        let be = ui.global::<Backend>();
        let note = match &status {
            btscan::Status::Starting => "SUCHE STARTET ...".to_string(),
            btscan::Status::Scanning if devs.is_empty() && now > self.radar_started + 8000 => "KEINE GERÄTE\nIN REICHWEITE".into(),
            btscan::Status::Scanning => String::new(),
            btscan::Status::BtOff => "BLUETOOTH IST AUS\nEINSTELLUNGEN › BLUETOOTH".into(),
            btscan::Status::NoAdapter => "KEIN BLUETOOTH-ADAPTER".into(),
            btscan::Status::NoService => "BLUETOOTH-DIENST\nLÄUFT NICHT".into(),
            btscan::Status::Error(e) => format!("FEHLER\n{}", e.to_uppercase()),
        };
        be.set_radar_note(note.into());
        be.set_radar_head(match devs.len() {
            1 => "1 GERÄT".into(),
            n => format!("{n} GERÄTE").into(),
        });
        // keep the selection on the same device while the order changes
        let sel = self.radar_sel.as_ref().and_then(|k| devs.iter().position(|(d, _)| &d.path == k));
        let sel = match sel {
            Some(i) => i,
            None => {
                self.radar_sel = devs.first().map(|(d, _)| d.path.clone());
                0
            }
        };
        if self.radar_items.row_count() == items.len() {
            for (i, it) in items.iter().enumerate() {
                if self.radar_items.row_data(i).as_ref() != Some(it) {
                    self.radar_items.set_row_data(i, it.clone());
                }
            }
        } else {
            self.radar_items.set_vec(items);
        }
        be.set_radar_sel(sel as i32);
        self.radar_devs = devs;
    }

    fn radar_frame(&mut self) {
        let now = mono_ms();
        let active = self.radar_scan.as_ref().map(|s| s.status() == btscan::Status::Scanning).unwrap_or(false);
        // without a scan the picture is static: redraw it once a second
        if !active && !self.radar_full && now < self.radar_static_at + 1000 {
            return;
        }
        self.radar_static_at = now;
        let sweep = ((now % radar::PERIOD_MS) * 65536 / radar::PERIOD_MS) as u16;
        let r = self.radar_renderer.get_or_insert_with(radar::Renderer::new);
        r.frame(sweep, now, &self.radar_blips, self.radar_sel.as_deref(), active);
        // Tiles under the beam (and those it just left) every frame, all
        // tiles every eighth frame for the fading blips. Note: Slint's DRM
        // backend uses three buffers, so its software renderer currently
        // repaints the whole screen on every change anyway; the tiles only
        // pay off once it does partial repaints there.
        let trail = if active { radar::Renderer::trail_sectors(sweep) } else { 0 };
        let mask = trail | self.radar_prev_trail;
        self.radar_prev_trail = trail;
        self.radar_frame_n += 1;
        let full = self.radar_tiles.row_count() != radar::TILES || self.radar_full || !active;
        self.radar_full = false;
        if self.radar_tiles.row_count() != radar::TILES {
            self.radar_tiles.set_vec(vec![slint::Image::default(); radar::TILES]);
        }
        for t in 0..radar::TILES {
            if full || r.tile_sectors(t) & mask != 0 || self.radar_frame_n % 8 == 0 {
                self.radar_tiles.set_row_data(t, slint::Image::from_rgb8(r.tile(t)));
            }
        }
    }

    fn radar_tap(&mut self, x: f32, y: f32) {
        let x = x - radar::OFFSET_X;
        let best = self
            .radar_blips
            .iter()
            .map(|b| (b, (b.x - x).powi(2) + (b.y - y).powi(2)))
            .filter(|(_, d)| *d <= 18.0 * 18.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(b, _)| b.key.clone());
        if let Some(k) = best {
            self.radar_sel = Some(k.clone());
            self.push(Page::RadarDev(k));
        } else if self.be().get_radar_note().contains("BLUETOOTH IST AUS") {
            self.push(Page::Bt);
        }
    }

    /// Vendor lookup for public addresses (reads the IEEE list once per device).
    fn radar_oui(&mut self, key: String) {
        let Some(d) = self.radar_scan.as_ref().and_then(|s| s.device(&key)) else { return };
        if d.addr_type == "random" || d.addr.is_empty() || self.oui_cache.contains_key(&d.addr) {
            return;
        }
        let addr = d.addr.clone();
        bg(
            move || {
                let v = radar::oui_vendor(&addr);
                (addr, v)
            },
            |a, (addr, v)| {
                a.oui_cache.insert(addr, v);
                a.refresh();
            },
        );
    }

    fn rows_radar_dev(&self, key: &str) -> Vec<Row> {
        let live = self.radar_scan.as_ref().and_then(|s| s.device(key));
        let Some(d) = live.or_else(|| self.radar_devs.iter().find(|(d, _)| d.path == key).map(|x| x.0.clone())) else {
            return vec![info("STATUS", "GERÄT NICHT MEHR DA")];
        };
        let now = mono_ms();
        let yn = |b: bool| if b { "JA" } else { "NEIN" };
        let mut v = vec![sep("GERÄT")];
        v.push(info("NAME", if d.name.is_empty() { "–" } else { &d.name }));
        if !d.alias.is_empty() && d.alias != d.name && d.alias.replace('-', ":") != d.addr {
            v.push(info("ALIAS", &d.alias));
        }
        v.push(info("ADRESSE", &d.addr));
        v.push(info("ADRESSTYP", radar::addr_type_de(&d)));
        if d.addr_type != "random" {
            let vendor = match self.oui_cache.get(&d.addr) {
                None => "SUCHE ...".to_string(),
                Some(Some(n)) => n.clone(),
                Some(None) if std::path::Path::new("/usr/share/ieee-data/oui.txt").exists() => "UNBEKANNT".into(),
                Some(None) => "LISTE FEHLT (IEEE-DATA)".into(),
            };
            v.push(info("HERSTELLER (OUI)", &vendor));
        }
        v.push(info("GERÄTETYP", &radar::dev_type(&d).unwrap_or_else(|| "UNBEKANNT".into())));
        if let Some(k) = radar::apple_kind_of(&d) {
            v.push(info("APPLE-SIGNAL", k));
        }
        if let Some(a) = d.appearance {
            v.push(info("APPEARANCE", &format!("0x{a:04X}")));
        }
        if let Some(c) = d.class {
            v.push(info("GERÄTEKLASSE", &format!("0x{c:06X}")));
        }

        v.push(sep("SIGNAL"));
        let cur = d.rssi.back().map(|s| s.1);
        let smooth = d.rssi_smooth(now, radar::SMOOTH_MS);
        v.push(info("RSSI", &cur.map(|r| format!("{r} DBM")).unwrap_or_else(|| "–".into())));
        if let Some(s) = smooth {
            v.push(info("RSSI GEGLÄTTET", &format!("{s} DBM (MEDIAN)")));
        }
        let hist: Vec<i16> = d.rssi.iter().rev().take(26).rev().map(|s| s.1).collect();
        if !hist.is_empty() {
            v.push(info("VERLAUF", &radar::sparkline(&hist)));
        }
        v.push(info("TXPOWER", &d.tx_power.map(|t| format!("{t} DBM")).unwrap_or_else(|| "–".into())));
        if let Some(s) = smooth {
            v.push(info("ZONE", radar::zone(s)));
            v.push(info(
                "ENTFERNUNG",
                &radar::distance_m(s, d.tx_power).map(|m| format!("{} (GESCHÄTZT)", radar::fmt_distance(m))).unwrap_or_else(|| "NUR ZONE (KEIN TXPOWER)".into()),
            ));
        }
        v.push(info("RICHTUNG", "NICHT MESSBAR"));

        if !d.manuf.is_empty() {
            v.push(sep("HERSTELLERDATEN"));
            for (id, data) in &d.manuf {
                v.push(info(&format!("0x{id:04X}"), &radar::company(*id)));
                for (i, l) in radar::hex_lines(data, 12).iter().enumerate() {
                    v.push(info(if i == 0 { "  DATEN" } else { "" }, l));
                }
            }
        }
        if !d.uuids.is_empty() {
            v.push(sep(&format!("DIENSTE ({})", d.uuids.len())));
            for u in &d.uuids {
                v.push(info(radar::uuid_name(u).unwrap_or("UNBEKANNT"), &radar::uuid_short(u)));
            }
        }
        if !d.svc_data.is_empty() {
            v.push(sep("DIENSTDATEN"));
            for (u, data) in &d.svc_data {
                v.push(info(radar::uuid_name(u).unwrap_or("UNBEKANNT"), &radar::uuid_short(u)));
                for (i, l) in radar::hex_lines(data, 12).iter().enumerate() {
                    v.push(info(if i == 0 { "  DATEN" } else { "" }, l));
                }
            }
        }

        v.push(sep("STATUS"));
        v.push(info("GEKOPPELT", yn(d.paired)));
        v.push(info("VERBUNDEN", yn(d.connected)));
        v.push(info("VERTRAUT", yn(d.trusted)));
        v.push(info("BLOCKIERT", yn(d.blocked)));

        v.push(sep("GESEHEN"));
        v.push(info("ZUERST", &fmt_wall(d.first_wall)));
        v.push(info("ZULETZT", &format!("{} (VOR {} S)", fmt_wall(d.last_wall), now.saturating_sub(d.last_seen) / 1000)));
        v.push(sep("HINWEIS"));
        v.push(info("RICHTUNG", "NICHT MESSBAR"));
        v.push(info("NUR ENTFERNUNG", "GESCHÄTZT"));
        v.push(info("WINKEL AUF DEM RADAR", "ERFUNDEN"));
        v
    }

    fn rows_update(&self) -> Vec<Row> {
        let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
        unsafe { libc::uname(&mut uts) };
        let kernel = unsafe { std::ffi::CStr::from_ptr(uts.release.as_ptr()) }.to_string_lossy().into_owned();
        let p = &self.upd_progress;
        let mut v = vec![
            info("SYSTEM", &update::os_name().to_uppercase()),
            info("KERNEL", &kernel),
            info("ZULETZT GEPRÜFT", if self.upd_last_check.is_empty() { "NOCH NICHT" } else { &self.upd_last_check }),
        ];
        if p.running {
            v.push(sep("UPDATE LÄUFT"));
            let total = self.upd_total.max(p.done);
            v.push(info("FORTSCHRITT", &if total > 0 { format!("{} / {} PAKETE", p.done, total) } else { format!("{} PAKETE", p.done) }));
            v.push(info("", &p.last_line));
            v.push(info("HINWEIS", "NICHT AUSSCHALTEN"));
            return v;
        }
        let busy = self.upd_checking;
        v.push(row("check", "AUF UPDATES PRÜFEN", if busy { "PRÜFE ..." } else { "" }, 0, !busy));
        if let Some(pkgs) = &self.upd_pkgs {
            v.push(info("VERFÜGBAR", &format!("{} PAKETE", pkgs.len())));
            v.push(row("upgrade", "JETZT AKTUALISIEREN", "", 0, !pkgs.is_empty() && !busy));
        }
        if let Some(code) = p.exit {
            v.push(info("LETZTES UPDATE", if code == 0 { "ERFOLGREICH" } else { "FEHLGESCHLAGEN" }));
            if code != 0 && !p.last_line.is_empty() {
                v.push(info("", &p.last_line));
            }
        }
        if update::reboot_required() {
            v.push(action("reboot", "NEUSTART", "EMPFOHLEN"));
        }
        if let Some(pkgs) = self.upd_pkgs.as_ref().filter(|p| !p.is_empty()) {
            v.push(sep(&format!("PAKETE ({})", pkgs.len())));
            for (name, ver) in pkgs.iter().take(60) {
                v.push(info(name, ver));
            }
        }
        v.push(sep("HINWEIS"));
        v.push(info("NEUER KERNEL", "AKTIV NACH NEUSTART"));
        v
    }

    fn update_check(&mut self) {
        if self.upd_checking {
            return;
        }
        self.upd_checking = true;
        self.message("LADE PAKETLISTEN ...", 120);
        self.refresh();
        bg(update::check, |a, r| {
            a.upd_checking = false;
            let t = timecfg::now();
            a.upd_last_check = format!("{:02}.{:02}. {:02}:{:02}", t.day, t.month, t.hour, t.minute);
            match r {
                Ok(p) => {
                    a.message(&if p.is_empty() { "SYSTEM IST AKTUELL".to_string() } else { format!("{} UPDATES VERFÜGBAR", p.len()) }, 4);
                    a.upd_pkgs = Some(p);
                }
                Err(e) => a.message(&format!("FEHLER: {e}"), 8),
            }
            a.refresh();
        });
    }

    fn update_start(&mut self) {
        self.upd_total = self.upd_pkgs.as_ref().map(|p| p.len()).unwrap_or(0);
        self.message("UPDATE STARTET ...", 5);
        bg(update::start, |a, r| {
            match r {
                Ok(()) => {
                    a.upd_progress.running = true;
                    a.upd_progress.exit = None;
                }
                Err(e) => a.message(&format!("FEHLER: {e}"), 8),
            }
            a.refresh();
            a.update_poll();
        });
    }

    fn update_poll(&mut self) {
        if self.upd_poll_busy {
            return;
        }
        self.upd_poll_busy = true;
        bg(update::progress, |a, p| {
            a.upd_poll_busy = false;
            let finished = a.upd_progress.running && !p.running;
            a.upd_progress = p;
            if finished {
                let ok = a.upd_progress.exit == Some(0);
                a.message(if ok { "UPDATE ABGESCHLOSSEN" } else { "UPDATE FEHLGESCHLAGEN" }, 8);
                // the list is stale now
                a.upd_pkgs = None;
            }
            if matches!(a.page(), Page::Update | Page::Menu) {
                a.refresh();
            }
        });
    }

    // ---- upload ----------------------------------------------------------------

    fn rows_upload(&self) -> Vec<Row> {
        let wlan_ip = self.ips.iter().find(|(d, _)| d.starts_with("wl")).or(self.ips.first()).map(|(_, ip)| ip.clone());
        let mut v = vec![row("upl", "WEBSERVER", if self.upload_on { "AN" } else { "AUS" }, 1, !self.upload_busy)];
        if self.upload_on {
            v.push(info("ADRESSE", &match &wlan_ip {
                Some(ip) => format!("http://{ip}:{}", upload::PORT),
                None => "KEIN NETZ".into(),
            }));
            v.push(info("PIN", &self.upload_pin));
        }
        v.push(info("ORDNER", "~/games"));
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if let Ok(c) = std::ffi::CString::new(upload::root()) {
            if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
                v.push(info("FREI", &sys::fmt_bytes(st.f_bavail as u64 * st.f_frsize as u64)));
            }
        }
        v.push(sep("SO GEHT'S"));
        v.push(info("1. BROWSER", "ADRESSE ÖFFNEN"));
        v.push(info("2. PIN", "EINGEBEN"));
        v.push(info("3. HOCHLADEN", "DATEIEN / ORDNER"));
        v.push(info("SCUMMVM", "SPIELORDNER NACH SCUMMVM/"));
        v.push(info("DANACH", "WEBSERVER AUSSCHALTEN"));
        v
    }

    fn upload_poll(&mut self) {
        bg(
            || {
                let on = upload::active();
                (on, if on { upload::running_pin() } else { None })
            },
            |a, (on, pin)| {
                a.upload_on = on;
                if let Some(p) = pin {
                    a.upload_pin = p;
                }
                if a.page() == Page::Upload {
                    a.refresh();
                }
            },
        );
    }

    // ---- terminal --------------------------------------------------------

    /// (rows, cols, font px, char width px, line height px) of the terminal
    /// area (476 x 218 px).
    fn term_geom(&self) -> (u16, u16, f32, f32, f32) {
        if self.settings.term_big {
            (15, 65, 12.0, 7.2246, 14.0)
        } else {
            (18, 79, 10.0, 6.0205, 12.0)
        }
    }

    fn term_apply_geom(&mut self) {
        let (rows, cols, font, cw, lh) = self.term_geom();
        let be = self.be();
        be.set_term_font(font);
        be.set_term_cw(cw);
        be.set_term_lh(lh);
        if let Some(t) = self.term.as_mut() {
            t.resize(rows, cols);
        }
        self.term_redraw();
    }

    fn term_open(&mut self) {
        if self.term.as_ref().map(|t| t.alive()).unwrap_or(false) {
            self.term_redraw();
            return;
        }
        self.term_apply_geom();
        let (rows, cols, ..) = self.term_geom();
        match term::Term::spawn(rows, cols) {
            Ok(t) => {
                if let Some(fd) = t.reader_fd() {
                    let pid = t.pid;
                    term::spawn_reader(fd, move |data| {
                        let _ = slint::invoke_from_event_loop(move || with_app(move |a| a.term_output(pid, data)));
                    });
                }
                self.term = Some(t);
                self.term_redraw();
            }
            Err(e) => self.message(&format!("FEHLER: {e}"), 6),
        }
    }

    fn term_output(&mut self, pid: libc::pid_t, data: Option<Vec<u8>>) {
        let Some(t) = self.term.as_mut().filter(|t| t.pid == pid) else { return };
        match data {
            Some(bytes) => {
                t.parser.process(&bytes);
                // coalesce bursts of output into one redraw
                if !self.term_redraw_pending {
                    self.term_redraw_pending = true;
                    slint::Timer::single_shot(Duration::from_millis(40), || {
                        with_app(|a| {
                            a.term_redraw_pending = false;
                            a.term_redraw();
                        })
                    });
                }
            }
            None => {
                self.term = None;
                self.term_segs.set_vec(Vec::new());
                if self.page() == Page::Term {
                    self.back();
                    self.message("SHELL BEENDET", 3);
                }
            }
        }
    }

    fn term_redraw(&mut self) {
        if self.page() != Page::Term {
            return;
        }
        let Some(t) = self.term.as_ref() else { return };
        let segs: Vec<TermSeg> = t
            .segments()
            .into_iter()
            .map(|s| TermSeg { row: s.row as i32, col: s.col as i32, text: s.text.into(), inv: s.inverse, bold: s.bold })
            .collect();
        self.term_segs.set_vec(segs);
    }

    fn term_key(&mut self, text: String, ctrl: bool) {
        let ctrl = ctrl || self.term_ctrl;
        let Some(t) = self.term.as_mut() else { return };
        let bytes = t.encode_key(&text, ctrl);
        if bytes.is_empty() {
            return; // a bare modifier key: keep a pending CTRL
        }
        t.write(&bytes);
        if self.term_ctrl {
            self.term_ctrl = false;
            self.be().set_term_ctrl(false);
        }
        self.term_redraw();
    }

    fn term_soft(&mut self, id: String) {
        use slint::platform::Key;
        match id.as_str() {
            "ctrl" => {
                self.term_ctrl = !self.term_ctrl;
                self.be().set_term_ctrl(self.term_ctrl);
            }
            "esc" => self.term_key(char::from(Key::Escape).to_string(), false),
            "tab" => self.term_key(char::from(Key::Tab).to_string(), false),
            "up" => self.term_key(char::from(Key::UpArrow).to_string(), false),
            "down" => self.term_key(char::from(Key::DownArrow).to_string(), false),
            _ => {}
        }
    }

    fn term_scroll(&mut self, lines: i32) {
        if let Some(t) = self.term.as_mut() {
            t.scroll(lines as isize);
        }
        self.term_redraw();
    }

    /// Low battery: warn once per discharge, power off with a countdown.
    /// pocketchip-battery-guard.service is the fallback when the console
    /// is not running (e.g. during an emulator).
    fn check_battery(&mut self) {
        let b = &self.bat;
        let on_battery = b.present && b.capacity >= 0 && !b.external_power() && !b.charging();
        let cap = b.capacity;
        let (warn, off) = (self.settings.battery_warn, self.settings.battery_off);
        if !on_battery {
            if self.bat_off_in.take().is_some() {
                if self.be().get_dialog_id() == "lowbat_off" {
                    self.be().set_dialog_open(false);
                    self.ui.invoke_refocus();
                }
                self.message("LADEGERÄT ERKANNT, AUSSCHALTEN ABGEBROCHEN", 5);
            }
            self.bat_warned = false;
            return;
        }
        if warn > 0 && cap > warn + 2 {
            self.bat_warned = false;
        }
        if off > 0 && cap <= off {
            let left = match self.bat_off_in {
                None => 30,
                Some(n) => n.saturating_sub(1),
            };
            self.bat_off_in = Some(left);
            if left == 0 {
                self.bat_off_in = None;
                self.do_power("poweroff");
                return;
            }
            self.wake_for_alert();
            let text = format!("NOCH {cap} %. DAS SYSTEM FÄHRT IN {left} S HERUNTER. LADEGERÄT ANSCHLIESSEN ZUM ABBRECHEN.");
            if self.be().get_dialog_open() && self.be().get_dialog_id() == "lowbat_off" {
                self.be().set_dialog_text(text.into());
            } else {
                self.dialog("lowbat_off", "AKKU LEER", &text, &[("JETZT AUSSCHALTEN", "now")], 0);
            }
            return;
        }
        if warn > 0 && cap <= warn && !self.bat_warned {
            self.bat_warned = true;
            self.wake_for_alert();
            let off_text = if off > 0 { format!(" BEI {off} % FÄHRT DAS SYSTEM HERUNTER.") } else { String::new() };
            let text = format!("NOCH {cap} %. BITTE LADEGERÄT ANSCHLIESSEN.{off_text}");
            self.dialog("lowbat", "AKKU SCHWACH", &text, &[("OK", "no")], 0);
        }
    }

    /// Make an alert visible: leave standby / switch the display on.
    fn wake_for_alert(&mut self) {
        if self.standby {
            self.wake_standby();
        } else if self.screen_off {
            self.screen_set(true);
        }
        hw::LAST_ACTIVITY.store(mono_ms(), Ordering::Relaxed);
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
        if let Some(f) = self.bat_fake {
            // debug control file: `fakebat N` / `fakebat charge` / `fakebat off`
            if f < 0 {
                self.bat.usb_online = true;
                self.bat.status = "Charging".into();
            } else {
                self.bat.capacity = f;
                self.bat.usb_online = false;
                self.bat.ac_online = false;
                self.bat.status = "Discharging".into();
            }
        }
        self.be().set_battery(self.bat.short().into());
        self.check_battery();
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
        if self.tick_n % 2 == 0 && (self.upd_progress.running || self.page() == Page::Update) {
            self.update_poll();
        }
        if self.tick_n % 10 == 0 && self.page() == Page::Upload {
            self.ips = sys::ip_addrs();
            self.upload_poll();
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
        if matches!(self.page(), Page::Radar | Page::RadarDev(_)) {
            self.radar_sync();
            self.radar_update();
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
        if sb > 0 && idle >= sb && allowed && !self.standby && !self.upd_progress.running && now > self.suspending_until {
            self.suspend();
        }

        // pages with live values
        match self.page() {
            Page::Menu | Page::Status | Page::Battery | Page::Time | Page::Overlay(_) | Page::RadarDev(_) => {
                if !self.be().get_dialog_open() {
                    self.refresh()
                }
            }
            _ => {}
        }
    }
}

/// Local time of day for a unix timestamp.
fn fmt_wall(t: i64) -> String {
    if t <= 0 {
        return "–".into();
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let tt = t as libc::time_t;
    unsafe { libc::localtime_r(&tt, &mut tm) };
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
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
    let term_segs = Rc::new(VecModel::<TermSeg>::default());
    let radar_items = Rc::new(VecModel::<RadarItem>::default());
    let radar_tiles = Rc::new(VecModel::<slint::Image>::default());
    let be = ui.global::<Backend>();
    be.set_rows(ModelRc::from(rows.clone()));
    be.set_term_segs(ModelRc::from(term_segs.clone()));
    be.set_radar_items(ModelRc::from(radar_items.clone()));
    be.set_radar_tiles(ModelRc::from(radar_tiles.clone()));
    be.set_scanlines(settings.scanlines);

    be.on_activate(|id| with_app(move |a| a.activate(id.to_string())));
    be.on_adjust(|id, d| with_app(move |a| a.adjust(id.to_string(), d)));
    be.on_back(|| with_app(|a| a.back()));
    be.on_selected(|_| {});
    be.on_dialog_result(|id, i| with_app(move |a| a.dialog_result(id.to_string(), i)));
    be.on_input_done(|id, text, ok| with_app(move |a| a.input_done(id.to_string(), text.to_string(), ok)));
    be.on_term_key(|t, ctrl| with_app(move |a| a.term_key(t.to_string(), ctrl)));
    be.on_term_soft(|id| with_app(move |a| a.term_soft(id.to_string())));
    be.on_term_scroll(|n| with_app(move |a| a.term_scroll(n)));
    be.on_radar_tap(|x, y| with_app(move |a| a.radar_tap(x, y)));
    be.on_radar_select(|id| {
        with_app(move |a| {
            a.radar_sel = Some(id.to_string());
            a.radar_full = true;
        })
    });
    be.on_radar_open(|id| with_app(move |a| a.push(Page::RadarDev(id.to_string()))));
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
        bat_warned: false,
        bat_off_in: None,
        bat_fake: None,
        term: None,
        term_segs: term_segs.clone(),
        term_ctrl: false,
        term_redraw_pending: false,
        upd_pkgs: None,
        upd_checking: false,
        upd_last_check: String::new(),
        upd_progress: update::Progress::default(),
        upd_total: 0,
        upd_poll_busy: false,
        upload_on: false,
        upload_pin: String::new(),
        upload_busy: false,
        radar_scan: None,
        radar_started: 0,
        radar_renderer: None,
        radar_timer: slint::Timer::default(),
        radar_blips: Vec::new(),
        radar_devs: Vec::new(),
        radar_sel: None,
        radar_items: radar_items.clone(),
        radar_tiles: radar_tiles.clone(),
        radar_prev_trail: 0,
        radar_frame_n: 0,
        radar_static_at: 0,
        radar_full: true,
        oui_cache: HashMap::new(),
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    with_app(|a| {
        a.apply_led();
        a.update_poll();
        a.upload_poll();
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
