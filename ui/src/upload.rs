//! UPLOAD tile: starts/stops the small file server
//! (/usr/local/lib/pocketchip-console/pocketchip-upload) as its own unit.

use crate::util::{run, short_err};
use std::io::Read;

pub const PORT: u16 = 8080;
const UNIT: &str = "pocketchip-upload.service";
const PROG: &str = "/usr/local/lib/pocketchip-console/pocketchip-upload";

fn user_home(name: &str) -> Option<String> {
    let c = std::ffi::CString::new(name).ok()?;
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned())
    }
}

pub fn user() -> String {
    std::env::var("POCKETCHIP_USER").unwrap_or_else(|_| "chip".into())
}

/// Folder shared by the server.
pub fn root() -> String {
    format!("{}/games", user_home(&user()).unwrap_or_else(|| "/home/chip".into()))
}

pub fn new_pin() -> String {
    let mut b = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    format!("{:04}", u32::from_ne_bytes(b) % 10000)
}

pub fn active() -> bool {
    run("systemctl", &["is-active", "--quiet", UNIT]).0
}

/// PIN of a server that is already running (e.g. after a console restart).
pub fn running_pin() -> Option<String> {
    let (_, out, _) = run("systemctl", &["show", "-p", "Environment", "--value", UNIT]);
    out.split_whitespace().find_map(|kv| kv.strip_prefix("UPLOAD_PIN=").map(String::from))
}

pub fn start(pin: &str) -> Result<(), String> {
    let u = user();
    let home = user_home(&u).ok_or_else(|| format!("BENUTZER {u} FEHLT"))?;
    let _ = run("systemctl", &["reset-failed", UNIT]);
    let args = [
        "--unit".to_string(),
        UNIT.to_string(),
        "--description".to_string(),
        "PocketCHIP Upload-Webserver".to_string(),
        "--collect".to_string(),
        format!("--uid={u}"),
        format!("--gid={u}"),
        format!("--working-directory={home}"),
        format!("--setenv=HOME={home}"),
        format!("--setenv=UPLOAD_PIN={pin}"),
        format!("--setenv=UPLOAD_PORT={PORT}"),
        format!("--setenv=UPLOAD_ROOT={home}/games"),
        PROG.to_string(),
    ];
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, _, err) = run("systemd-run", &a);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn stop() -> Result<(), String> {
    let (ok, _, err) = run("systemctl", &["stop", UNIT]);
    if ok { Ok(()) } else { Err(short_err(&err)) }
}
