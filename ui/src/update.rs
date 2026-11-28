//! OS update through /usr/local/sbin/pocketchip-update (apt).

use crate::util::{read_trim, run, run_timeout, short_err};
use std::time::Duration;

const TOOL: &str = "/usr/local/sbin/pocketchip-update";
const UNIT: &str = "pocketchip-update.service";
const LOG: &str = "/var/log/pocketchip-update.log";

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub running: bool,
    /// packages set up so far
    pub done: usize,
    pub last_line: String,
    /// exit code of the last finished run (from the log)
    pub exit: Option<i32>,
}

pub fn os_name() -> String {
    read_trim("/etc/os-release")
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_else(|| "LINUX".into())
}

/// Refresh the package lists and return (package, new version).
pub fn check() -> Result<Vec<(String, String)>, String> {
    let (ok, out, err) = run_timeout(TOOL, &["check"], Duration::from_secs(600));
    if !ok {
        let msg = if out.trim().is_empty() { err } else { out };
        return Err(short_err(&msg));
    }
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            (f.next()? == "UPG").then_some(())?;
            Some((f.next()?.to_string(), f.next().unwrap_or("").to_string()))
        })
        .collect())
}

pub fn start() -> Result<(), String> {
    // a finished run leaves a failed unit behind; clear it first
    let _ = run("systemctl", &["reset-failed", UNIT]);
    let (ok, _, err) = run(
        "systemd-run",
        &["--unit", UNIT, "--description", "PocketCHIP Systemupdate", "--collect", TOOL, "upgrade"],
    );
    if ok { Ok(()) } else { Err(short_err(&err)) }
}

pub fn progress() -> Progress {
    let (running, _, _) = run("systemctl", &["is-active", "--quiet", UNIT]);
    let log = std::fs::read_to_string(LOG).unwrap_or_default();
    let done = log.lines().filter(|l| l.starts_with("Setting up ")).count();
    let exit = log.lines().rev().find_map(|l| l.strip_prefix("EXIT=").and_then(|v| v.trim().parse().ok()));
    let last_line = log
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("EXIT="))
        .unwrap_or("")
        .to_string();
    Progress { running, done, last_line, exit: if running { None } else { exit } }
}

pub fn reboot_required() -> bool {
    std::path::Path::new("/run/reboot-required").exists()
}
