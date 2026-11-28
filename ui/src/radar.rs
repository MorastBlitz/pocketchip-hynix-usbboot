//! RADAR page: draws the sweep display into a pixel buffer and turns the
//! BlueZ data into readable text.
//!
//! Honest geometry: there is one omnidirectional antenna. The distance from
//! the centre comes from the smoothed RSSI; the angle is made up (a hash of
//! the address), only so a device keeps its place.

use crate::btscan::Dev;
use slint::{Rgb8Pixel, SharedPixelBuffer};

pub const W: u32 = 212;
pub const H: u32 = 216;
/// left edge of the picture inside the 260 px radar area
pub const OFFSET_X: f32 = 24.0;
pub const CX: f32 = 106.0;
pub const CY: f32 = 108.0;
/// radius of the outer ring
pub const R: f32 = 92.0;
/// one turn of the beam
pub const PERIOD_MS: u64 = 4800;
/// frame interval (12 fps: every frame costs a full-screen repaint in Slint)
pub const FRAME_MS: u64 = 83;
/// devices not heard for this long are dropped (BlueZ reports an RSSI only
/// when it changed; devices it forgets after 30 s go at once)
pub const STALE_MS: u64 = 60_000;
/// shown dimmed when not heard for this long
pub const FRESH_MS: u64 = 25_000;
/// window for the median
pub const SMOOTH_MS: u64 = 60_000;

const FG: (f32, f32, f32) = (57.0, 255.0, 106.0);

/// Zone borders in dBm (stronger than the value = inside the zone).
pub const ZONES: [(i16, &str); 4] = [(-60, "NAH"), (-75, "MITTEL"), (-88, "FERN"), (i16::MIN, "SEHR FERN")];

pub fn zone(rssi: i16) -> &'static str {
    ZONES.iter().find(|(b, _)| rssi > *b).map(|z| z.1).unwrap_or("SEHR FERN")
}

/// Fraction of the radius for a signal level: -40 dBm near the centre,
/// -100 dBm at the outer ring; the zone borders fall onto the rings.
pub fn radius_frac(rssi: i16) -> f32 {
    ((-40.0 - rssi as f32) / 60.0).clamp(0.06, 1.0)
}

/// Distance estimate from the advertised TX power (log-distance path loss,
/// exponent 2.5 for rooms). 59 dB at 1 m: free space (41 dB) plus the usual
/// antenna and body losses, as in the common "measured power" of beacons.
pub fn distance_m(rssi: i16, tx_power: Option<i16>) -> Option<f32> {
    let tx = tx_power?;
    if !(-60..=20).contains(&tx) {
        return None;
    }
    let at_1m = tx as f32 - 59.0;
    Some(10f32.powf((at_1m - rssi as f32) / 25.0))
}

pub fn fmt_distance(m: f32) -> String {
    if m < 1.0 {
        "< 1 M".into()
    } else if m < 10.0 {
        format!("≈ {:.1} M", m)
    } else {
        format!("≈ {:.0} M", m)
    }
}

/// Stable made-up angle (0..65536 = full turn, 0 = up, clockwise).
pub fn angle_of(addr: &str) -> u16 {
    let mut h: u32 = 0x811c9dc5;
    for b in addr.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    (h ^ (h >> 16)) as u16
}

#[derive(Clone, Debug)]
pub struct Blip {
    pub key: String,
    pub x: f32,
    pub y: f32,
    pub angle: u16,
    /// mono ms of the first sighting (for the ping ring)
    pub first: u64,
    /// heard in the last 10 s
    pub fresh: bool,
}

pub fn blip_pos(angle: u16, frac: f32) -> (f32, f32) {
    let a = angle as f32 / 65536.0 * std::f32::consts::TAU;
    (CX + a.sin() * frac * R, CY - a.cos() * frac * R)
}

pub struct Renderer {
    bg: Vec<u8>,
    /// (angle, byte offset) of every pixel inside the outer ring, sorted by
    /// angle, so a frame only walks the pixels under the beam trail
    disc: Vec<(u16, u32)>,
    /// colour added by the beam trail, by angular distance (>> 6)
    trail: Vec<[u8; 3]>,
    /// blip sprite: (dx, dy, core coverage, halo coverage) in 1/256
    sprite: Vec<(i32, i32, u32, u32)>,
    /// the last frame
    buf: Vec<u8>,
    /// per tile: bit k set = the tile has disc pixels in sector k (64 sectors)
    tile_sectors: [u64; TILES],
}

/// The picture is shown as a grid of tiles, so Slint only repaints the
/// tiles that changed (the software renderer's cost grows with the area).
pub const TILES_X: u32 = 4;
pub const TILES_Y: u32 = 4;
pub const TILES: usize = (TILES_X * TILES_Y) as usize;
pub const TILE_W: u32 = W / TILES_X;
pub const TILE_H: u32 = H / TILES_Y;

/// Bit mask of the 64 sectors touched by the angle range (a0, a1].
fn sector_mask(a0: u16, a1: u16) -> u64 {
    let (s0, s1) = ((a0 >> 10) as u32, (a1 >> 10) as u32);
    let mut m = 0u64;
    let mut k = s0;
    loop {
        m |= 1 << k;
        if k == s1 {
            break;
        }
        k = (k + 1) % 64;
    }
    m
}

const TRAIL: u32 = 65536 / 4; // 90°

fn put(buf: &mut [u8], x: i32, y: i32, c: (f32, f32, f32), a: f32) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 || a <= 0.0 {
        return;
    }
    let i = ((y as u32 * W + x as u32) * 3) as usize;
    let a = a.min(1.0);
    for (k, ch) in [c.0, c.1, c.2].iter().enumerate() {
        let v = buf[i + k] as f32;
        buf[i + k] = (v + (ch - v) * a).clamp(0.0, 255.0) as u8;
    }
}


/// Anti-aliased circle outline.
fn ring(buf: &mut [u8], cx: f32, cy: f32, r: f32, c: (f32, f32, f32), a: f32) {
    let x0 = (cx - r - 2.0) as i32;
    let x1 = (cx + r + 2.0) as i32;
    let y0 = (cy - r - 2.0) as i32;
    let y1 = (cy + r + 2.0) as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let cov = 1.0 - (d - r).abs();
            if cov > 0.0 {
                put(buf, x, y, c, cov * a);
            }
        }
    }
}

/// Anti-aliased line (Wu-style sampling, good enough for thin lines).
fn line(buf: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, c: (f32, f32, f32), a: f32) {
    let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt().max(1.0);
    let n = (len * 2.0) as i32;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        let (xf, yf) = (x.floor(), y.floor());
        let (fx, fy) = (x - xf, y - yf);
        put(buf, xf as i32, yf as i32, c, a * (1.0 - fx) * (1.0 - fy) * 0.7);
        put(buf, xf as i32 + 1, yf as i32, c, a * fx * (1.0 - fy) * 0.7);
        put(buf, xf as i32, yf as i32 + 1, c, a * (1.0 - fx) * fy * 0.7);
        put(buf, xf as i32 + 1, yf as i32 + 1, c, a * fx * fy * 0.7);
    }
}

fn scale(c: (f32, f32, f32), f: f32) -> (f32, f32, f32) {
    (c.0 * f, c.1 * f, c.2 * f)
}

impl Renderer {
    pub fn new() -> Renderer {
        let mut bg = vec![0u8; (W * H * 3) as usize];
        let mut disc = Vec::new();
        for y in 0..H {
            for x in 0..W {
                let dx = x as f32 + 0.5 - CX;
                let dy = y as f32 + 0.5 - CY;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= R {
                    let a = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
                    disc.push((((a * 65536.0) as u32).min(65535) as u16, (y * W + x) * 3));
                    // faint phosphor fill, a little brighter towards the rim
                    let f = 0.035 + 0.025 * (d / R);
                    let i = ((y * W + x) * 3) as usize;
                    bg[i] = (FG.0 * f) as u8;
                    bg[i + 1] = (FG.1 * f) as u8;
                    bg[i + 2] = (FG.2 * f) as u8;
                }
            }
        }
        let dim = scale(FG, 0.55);
        let faint = scale(FG, 0.28);
        // spokes every 30°
        for k in 0..12 {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            let (s, c) = a.sin_cos();
            let w = if k % 3 == 0 { 0.6 } else { 0.35 };
            line(&mut bg, CX + s * 6.0, CY - c * 6.0, CX + s * R, CY - c * R, faint, w);
        }
        // zone rings
        for (b, _) in ZONES.iter().take(3) {
            ring(&mut bg, CX, CY, radius_frac(*b) * R, dim, 0.75);
        }
        ring(&mut bg, CX, CY, R, FG, 1.0);
        ring(&mut bg, CX, CY, R + 3.0, dim, 0.6);
        // compass scale: every 5°, longer every 10° and 30°
        for k in 0..72 {
            let a = k as f32 * std::f32::consts::TAU / 72.0;
            let (s, c) = a.sin_cos();
            let len = if k % 6 == 0 { 9.0 } else if k % 2 == 0 { 6.0 } else { 3.5 };
            let r0 = R + 3.0;
            line(&mut bg, CX + s * r0, CY - c * r0, CX + s * (r0 + len), CY - c * (r0 + len), if k % 6 == 0 { FG } else { dim }, 1.0);
        }
        // centre mark
        line(&mut bg, CX - 5.0, CY, CX + 5.0, CY, FG, 1.0);
        line(&mut bg, CX, CY - 5.0, CX, CY + 5.0, FG, 1.0);

        let trail = (0..(TRAIL >> 6) + 1)
            .map(|i| {
                let t = 1.0 - (i << 6) as f32 / TRAIL as f32;
                let mut f = t * t * 0.42;
                if i < 6 {
                    // bright leading edge
                    f = 0.9;
                }
                [(FG.0 * f) as u8, (FG.1 * f) as u8, (FG.2 * f) as u8]
            })
            .collect();
        disc.sort_unstable();
        let mut sprite = Vec::new();
        for dy in -6i32..=6 {
            for dx in -6i32..=6 {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let core = (2.4 + 0.5 - d).clamp(0.0, 1.0);
                let halo = (4.5 + 0.5 - d).clamp(0.0, 1.0) * 0.35;
                if core > 0.0 || halo > 0.0 {
                    sprite.push((dx, dy, (core * 256.0) as u32, (halo * 256.0) as u32));
                }
            }
        }
        let mut tile_sectors = [0u64; TILES];
        for &(a, j) in &disc {
            let px = j / 3;
            let t = ((px / W) / TILE_H) * TILES_X + (px % W) / TILE_W;
            tile_sectors[t as usize] |= 1 << (a >> 10);
        }
        // the outer scale lies outside the disc but never changes
        let buf = bg.clone();
        Renderer { bg, disc, trail, sprite, buf, tile_sectors }
    }

    /// Sectors under the beam trail for this sweep angle.
    pub fn trail_sectors(sweep: u16) -> u64 {
        sector_mask(sweep.wrapping_sub(TRAIL as u16 - 1), sweep)
    }

    pub fn tile_sectors(&self, t: usize) -> u64 {
        self.tile_sectors[t]
    }

    /// Copy of one tile of the last frame.
    pub fn tile(&self, t: usize) -> SharedPixelBuffer<Rgb8Pixel> {
        let (tx, ty) = (t as u32 % TILES_X, t as u32 / TILES_X);
        let mut pb = SharedPixelBuffer::<Rgb8Pixel>::new(TILE_W, TILE_H);
        let out = pb.make_mut_bytes();
        let row = (TILE_W * 3) as usize;
        for y in 0..TILE_H {
            let src = (((ty * TILE_H + y) * W + tx * TILE_W) * 3) as usize;
            out[y as usize * row..(y as usize + 1) * row].copy_from_slice(&self.buf[src..src + row]);
        }
        pb
    }

    /// Draw one frame into the internal buffer. `sweep`: beam angle
    /// (0..65536), `sel`: key of the selected device, `active`: false draws
    /// the screen without the beam.
    pub fn frame(&mut self, sweep: u16, now: u64, blips: &[Blip], sel: Option<&str>, active: bool) {
        let mut owned = std::mem::take(&mut self.buf);
        owned.copy_from_slice(&self.bg);
        let buf = &mut owned[..];
        if active {
            // pixels with angle in (sweep - TRAIL, sweep], possibly wrapping
            let lo = sweep.wrapping_sub(TRAIL as u16 - 1);
            let ranges: [(u16, u16); 2] = if lo <= sweep { [(lo, sweep), (1, 0)] } else { [(lo, u16::MAX), (0, sweep)] };
            for (a0, a1) in ranges {
                if a0 > a1 {
                    continue;
                }
                let s0 = self.disc.partition_point(|p| p.0 < a0);
                let s1 = self.disc.partition_point(|p| p.0 <= a1);
                for &(a, j) in &self.disc[s0..s1] {
                    let d = sweep.wrapping_sub(a) as u32;
                    let c = self.trail[(d >> 6) as usize];
                    let j = j as usize;
                    buf[j] = buf[j].saturating_add(c[0]);
                    buf[j + 1] = buf[j + 1].saturating_add(c[1]);
                    buf[j + 2] = buf[j + 2].saturating_add(c[2]);
                }
            }
        }
        for b in blips {
            // lit when the beam passes, fading until the next turn
            let phase = if active { sweep.wrapping_sub(b.angle) as f32 / 65536.0 } else { 0.3 };
            let mut lum = 0.3 + 0.7 * (1.0 - phase).powf(1.6);
            if !b.fresh {
                lum *= 0.6;
            }
            let l = (lum * 256.0) as u32;
            let (bx, by) = (b.x.round() as i32, b.y.round() as i32);
            for &(dx, dy, core, halo) in &self.sprite {
                let (x, y) = (bx + dx, by + dy);
                if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                    continue;
                }
                let a = ((core + halo).min(256) * l) >> 8;
                let j = ((y as u32 * W + x as u32) * 3) as usize;
                buf[j] = buf[j].saturating_add(((FG.0 as u32 * a) >> 8) as u8);
                buf[j + 1] = buf[j + 1].saturating_add(((FG.1 as u32 * a) >> 8) as u8);
                buf[j + 2] = buf[j + 2].saturating_add(((FG.2 as u32 * a) >> 8) as u8);
            }
            let age = now.saturating_sub(b.first);
            if age < 1500 {
                // ping ring of a new device
                let t = age as f32 / 1500.0;
                ring(buf, b.x, b.y, 4.0 + t * 14.0, FG, 1.0 - t);
            }
            if sel == Some(b.key.as_str()) {
                bracket(buf, b.x, b.y, 9.0);
            }
        }
        self.buf = owned;
    }
}

/// Target frame: four corner brackets.
fn bracket(buf: &mut [u8], x: f32, y: f32, h: f32) {
    let (x, y) = (x.round(), y.round());
    let l = 4.0;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let cx = x + sx * h;
        let cy = y + sy * h;
        for k in 0..=l as i32 {
            let k = k as f32;
            put(buf, (cx - sx * k) as i32, cy as i32, FG, 1.0);
            put(buf, cx as i32, (cy - sy * k) as i32, FG, 1.0);
        }
    }
}

// ---- text: names and types ------------------------------------------------------

/// Company name from the Bluetooth SIG list. Uses BlueZ's libbluetooth when
/// installed (complete list), otherwise a short built-in list.
pub fn company(id: u16) -> String {
    if let Some(n) = libbt_company(id) {
        return n;
    }
    let n = match id {
        0x0002 => "Intel",
        0x0006 => "Microsoft",
        0x000D => "Texas Instruments",
        0x000F => "Broadcom",
        0x001D => "Qualcomm",
        0x004C => "Apple",
        0x0059 => "Nordic Semiconductor",
        0x0075 => "Samsung",
        0x0087 => "Garmin",
        0x009E => "Bose",
        0x00E0 => "Google",
        0x012D => "Sony",
        0x027D => "Huawei",
        0x02E5 => "Espressif",
        0x038F => "Xiaomi",
        _ => "",
    };
    if n.is_empty() { format!("UNBEKANNT (0x{id:04X})") } else { n.to_string() }
}

fn libbt_company(id: u16) -> Option<String> {
    use std::sync::OnceLock;
    type F = unsafe extern "C" fn(libc::c_int) -> *const libc::c_char;
    static FUNC: OnceLock<Option<usize>> = OnceLock::new();
    let f = (*FUNC.get_or_init(|| unsafe {
        let h = libc::dlopen(c"libbluetooth.so.3".as_ptr(), libc::RTLD_NOW);
        if h.is_null() {
            return None;
        }
        let s = libc::dlsym(h, c"bt_compidtostr".as_ptr());
        (!s.is_null()).then_some(s as usize)
    }))?;
    let f: F = unsafe { std::mem::transmute(f) };
    let p = unsafe { f(id as libc::c_int) };
    if p.is_null() {
        return None;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned();
    // strip additions in other scripts, e.g. "Espressif Incorporated ( ... )"
    let s = match s.find(" (") {
        Some(i) if s[i..].chars().any(|c| !c.is_ascii()) => s[..i].to_string(),
        _ => s,
    };
    (!s.is_empty() && s != "internal use").then_some(s)
}

/// Short company name for the list ("Apple, Inc." -> "Apple").
pub fn company_short(id: u16) -> String {
    let n = company(id);
    let n = n.split(',').next().unwrap_or(&n).to_string();
    for suffix in [" Corporation", " Incorporated", " Inc.", " Inc", " Co. Ltd.", " Co., Ltd.", " International", " Electronics", " GmbH", " Ltd.", " AB", " A/S", " ASA", " AG"] {
        if let Some(s) = n.strip_suffix(suffix) {
            return s.to_string();
        }
    }
    n
}

/// GAP appearance category in plain German.
pub fn appearance_de(a: u16) -> Option<&'static str> {
    Some(match a >> 6 {
        1 => "TELEFON",
        2 => "COMPUTER",
        3 => "UHR",
        4 => "WANDUHR",
        5 => "DISPLAY",
        6 => "FERNBEDIENUNG",
        7 => "BRILLE",
        8 => "TRACKER / TAG",
        9 => "SCHLÜSSELANHÄNGER",
        10 => "MEDIENPLAYER",
        11 => "BARCODE-SCANNER",
        12 => "THERMOMETER",
        13 => "HERZFREQUENZ-SENSOR",
        14 => "BLUTDRUCKMESSER",
        15 => match a & 0x3f {
            1 => "TASTATUR",
            2 => "MAUS",
            3 => "JOYSTICK",
            4 => "GAMEPAD",
            5 => "GRAFIKTABLETT",
            _ => "EINGABEGERÄT",
        },
        16 => "BLUTZUCKERMESSER",
        17 => "LAUF-SENSOR",
        18 => "FAHRRAD-SENSOR",
        19 => "STEUERGERÄT",
        20 => "NETZWERKGERÄT",
        21 => "SENSOR",
        22 => "LEUCHTE",
        23 => "VENTILATOR",
        31 => "LICHTQUELLE",
        33 => "LAUTSPRECHER",
        34 => "AUDIOQUELLE",
        36 => "HAUSHALTSGERÄT",
        37 => "KOPFHÖRER",
        41 => "HÖRGERÄT",
        42 => "SPIELKONSOLE",
        49 => "PULSOXIMETER",
        50 => "WAAGE",
        81 => "OUTDOOR-SPORTGERÄT",
        _ => return None,
    })
}

/// Major class of a classic (BR/EDR) device.
pub fn class_de(c: u32) -> Option<&'static str> {
    Some(match (c >> 8) & 0x1f {
        1 => "COMPUTER",
        2 => "TELEFON",
        3 => "NETZWERK",
        4 => "AUDIO / VIDEO",
        5 => "EINGABEGERÄT",
        6 => "DRUCKER / KAMERA",
        7 => "WEARABLE",
        8 => "SPIELZEUG",
        9 => "GESUNDHEIT",
        _ => return None,
    })
}

/// Device type in plain text: appearance, class, icon.
pub fn dev_type(d: &Dev) -> Option<String> {
    if let Some(t) = d.appearance.and_then(appearance_de) {
        return Some(t.into());
    }
    if let Some(t) = d.class.and_then(class_de) {
        return Some(t.into());
    }
    match crate::bt::icon_de(&d.icon) {
        "UNBEKANNT" | "SONSTIGES" => None,
        t => Some(t.into()),
    }
}

/// Kind of an Apple advertisement (first byte of the manufacturer data).
fn apple_kind(data: &[u8]) -> Option<&'static str> {
    Some(match *data.first()? {
        0x02 => "IBEACON",
        0x05 => "AIRDROP",
        0x07 => "AIRPODS / BEATS",
        0x09 => "AIRPLAY",
        0x0A => "AIRPLAY",
        0x0C => "HANDOFF",
        0x0F => "NEARBY ACTION",
        0x10 => "NEARBY",
        0x12 => "FIND MY",
        _ => return None,
    })
}

/// Name for the list: name, else type and maker, else the address.
pub fn label(d: &Dev) -> String {
    if !d.name.is_empty() {
        return d.name.clone();
    }
    if let Some((0x004C, data)) = d.manuf.first() {
        if let Some(k) = apple_kind(data) {
            return format!("APPLE {k}");
        }
    }
    let maker = d.manuf.first().map(|m| company_short(m.0).to_uppercase());
    // Microsoft beacons come from Windows PCs (device discovery)
    if d.manuf.first().map(|m| m.0) == Some(0x0006) && dev_type(d).is_none() {
        return "WINDOWS-PC".into();
    }
    match (dev_type(d), maker) {
        (Some(t), Some(m)) => format!("{m} {t}"),
        (Some(t), None) => t,
        (None, Some(m)) => format!("{m}-GERÄT"),
        (None, None) => d.addr.clone(),
    }
}

/// Apple advertisement kind for the detail page.
pub fn apple_kind_of(d: &Dev) -> Option<&'static str> {
    d.manuf.iter().find(|m| m.0 == 0x004C).and_then(|m| apple_kind(&m.1))
}

/// Address type in plain text; random addresses by their top two bits.
pub fn addr_type_de(d: &Dev) -> &'static str {
    if d.addr_type != "random" {
        return "ÖFFENTLICH";
    }
    let top = u8::from_str_radix(d.addr.get(0..2).unwrap_or("00"), 16).unwrap_or(0) >> 6;
    match top {
        0b11 => "ZUFÄLLIG STATISCH",
        0b01 => "ZUFÄLLIG PRIVAT (AUFLÖSBAR)",
        0b00 => "ZUFÄLLIG PRIVAT",
        _ => "ZUFÄLLIG",
    }
}

/// 16-bit UUID of a Bluetooth base UUID, if it is one.
pub fn uuid16(u: &str) -> Option<u16> {
    let u = u.to_ascii_lowercase();
    if u.len() == 36 && u.starts_with("0000") && u.ends_with("-0000-1000-8000-00805f9b34fb") {
        u16::from_str_radix(&u[4..8], 16).ok()
    } else {
        None
    }
}

/// Plain name of well known service UUIDs.
pub fn uuid_name(u: &str) -> Option<&'static str> {
    let low = u.to_ascii_lowercase();
    if low == "6e400001-b5a3-f393-e0a9-e50e24dcca9e" {
        return Some("NORDIC UART");
    }
    if low == "d0611e78-bbb4-4591-a5f8-487910ae4366" {
        return Some("APPLE CONTINUITY");
    }
    if low == "9fa480e0-4967-4542-9390-d343dc5d04ae" {
        return Some("APPLE NEARBY");
    }
    Some(match uuid16(u)? {
        0x1101 => "SERIELLER PORT",
        0x1105 => "OBJEKTÜBERTRAGUNG",
        0x1106 => "DATEIÜBERTRAGUNG",
        0x1108 | 0x1112 => "HEADSET",
        0x110A => "AUDIOQUELLE",
        0x110B => "AUDIO (A2DP)",
        0x110C | 0x110E | 0x110F => "FERNSTEUERUNG (AVRCP)",
        0x110D => "AUDIO (A2DP)",
        0x111E | 0x111F => "FREISPRECHEN",
        0x1115 | 0x1116 => "NETZWERK (PAN)",
        0x1124 => "EINGABEGERÄT (HID)",
        0x112F => "TELEFONBUCH",
        0x1132 => "NACHRICHTEN",
        0x1200 => "GERÄTE-ID",
        0x1800 => "GENERIC ACCESS",
        0x1801 => "GENERIC ATTRIBUTE",
        0x1802 => "SOFORT-ALARM",
        0x1803 => "VERBINDUNGSVERLUST",
        0x1804 => "SENDELEISTUNG",
        0x1805 => "AKTUELLE ZEIT",
        0x1809 => "THERMOMETER",
        0x180A => "GERÄTEINFORMATION",
        0x180D => "HERZFREQUENZ",
        0x180F => "AKKU",
        0x1810 => "BLUTDRUCK",
        0x1812 => "EINGABEGERÄT (HID)",
        0x1814 => "LAUFEN",
        0x1816 => "RADFAHREN",
        0x1818 => "RADLEISTUNG",
        0x1819 => "POSITION",
        0x181A => "UMWELTSENSOR",
        0x181C => "BENUTZERDATEN",
        0x181D => "WAAGE",
        0x1822 => "PULSOXIMETER",
        0x1826 => "FITNESSGERÄT",
        0x1827 | 0x1828 => "MESH",
        0x183B => "BINÄRSENSOR",
        0x1843 | 0x1844 | 0x1845 | 0x1846 | 0x184E | 0x184F | 0x1850 | 0x1851 | 0x1852 | 0x1853 | 0x1854 | 0x1855 | 0x1856 => {
            "LE AUDIO"
        }
        0xFD6F => "CORONA-WARN (EXPOSURE)",
        0xFE2C => "GOOGLE FAST PAIR",
        0xFE9F => "GOOGLE",
        0xFEAA => "EDDYSTONE-BEACON",
        0xFEED | 0xFEEC => "TILE-TRACKER",
        0xFE07 => "SONOS",
        0xFE59 => "NORDIC DFU",
        0xFE8F => "CSR",
        0xFEBE => "BOSE",
        0xFE03 => "AMAZON",
        0xFE0F => "PHILIPS HUE",
        0xFE95 => "XIAOMI",
        0xFEF3 => "GOOGLE",
        0xFD5A => "SAMSUNG",
        0xFD44 | 0xFD43 => "APPLE",
        0xFDF0 => "GOOGLE",
        _ => return None,
    })
}

pub fn uuid_short(u: &str) -> String {
    match uuid16(u) {
        Some(n) => format!("0x{n:04X}"),
        None => u.to_ascii_uppercase(),
    }
}

/// Hex dump in groups of four bytes, split into lines of `per_line` bytes.
pub fn hex_lines(b: &[u8], per_line: usize) -> Vec<String> {
    if b.is_empty() {
        return vec!["–".into()];
    }
    b.chunks(per_line)
        .map(|c| c.chunks(4).map(|g| g.iter().map(|x| format!("{x:02X}")).collect::<String>()).collect::<Vec<_>>().join(" "))
        .collect()
}

/// Small RSSI graph from block characters (-100 .. -40 dBm).
pub fn sparkline(v: &[i16]) -> String {
    const B: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    v.iter()
        .map(|&r| {
            let f = ((r as f32 + 100.0) / 60.0).clamp(0.0, 0.999);
            B[(f * 8.0) as usize]
        })
        .collect()
}

/// Vendor of a public address from the IEEE list (package ieee-data).
pub fn oui_vendor(addr: &str) -> Option<String> {
    let key = addr.get(0..8)?.replace(':', "-").to_ascii_uppercase();
    let f = std::fs::File::open("/usr/share/ieee-data/oui.txt").ok()?;
    use std::io::BufRead;
    for l in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        if l.starts_with(&key) && l.contains("(hex)") {
            return l.split("(hex)").nth(1).map(|v| v.trim().to_string());
        }
    }
    None
}
