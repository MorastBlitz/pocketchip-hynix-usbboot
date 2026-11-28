//! Touch calibration: fit a libinput calibration matrix from raw samples and
//! store it for libinput (udev property: the console on KMS, SDL apps) and the
//! shell tools (/etc/default/pocketchip-touch).

use crate::util::run;

pub const TARGETS: [(f64, f64); 5] = [(40.0, 40.0), (440.0, 40.0), (440.0, 232.0), (40.0, 232.0), (240.0, 136.0)];
pub const W: f64 = 480.0;
pub const H: f64 = 272.0;

const DEFAULTS: &str = "/etc/default/pocketchip-touch";
const UDEV: &str = "/etc/udev/rules.d/90-pocketchip-touch-libinput.rules";

fn det3(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Least squares a*x + b*y + c = t.
fn fit(raw: &[(f64, f64)], t: &[f64]) -> Option<[f64; 3]> {
    let mut a = [[0.0; 3]; 3];
    let mut r = [0.0; 3];
    for (i, &(x, y)) in raw.iter().enumerate() {
        let v = [x, y, 1.0];
        for j in 0..3 {
            for k in 0..3 {
                a[j][k] += v[j] * v[k];
            }
            r[j] += v[j] * t[i];
        }
    }
    let d = det3(a);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut out = [0.0; 3];
    for c in 0..3 {
        let mut m = a;
        for row in 0..3 {
            m[row][c] = r[row];
        }
        out[c] = det3(m) / d;
    }
    Some(out)
}

pub struct Result {
    pub matrix: [f64; 6],
    /// mean and max error of the fit in screen pixels
    pub mean_px: f64,
    pub max_px: f64,
}

pub fn compute(raw: &[(f64, f64)]) -> Option<Result> {
    if raw.len() != TARGETS.len() {
        return None;
    }
    let tx: Vec<f64> = TARGETS.iter().map(|t| t.0 / W).collect();
    let ty: Vec<f64> = TARGETS.iter().map(|t| t.1 / H).collect();
    let x = fit(raw, &tx)?;
    let y = fit(raw, &ty)?;
    let matrix = [x[0], x[1], x[2], y[0], y[1], y[2]];
    let mut sum = 0.0;
    let mut max: f64 = 0.0;
    for (i, &(rx, ry)) in raw.iter().enumerate() {
        let px = (x[0] * rx + x[1] * ry + x[2]) * W;
        let py = (y[0] * rx + y[1] * ry + y[2]) * H;
        let e = ((px - TARGETS[i].0).powi(2) + (py - TARGETS[i].1).powi(2)).sqrt();
        sum += e;
        max = max.max(e);
    }
    Some(Result { matrix, mean_px: sum / raw.len() as f64, max_px: max })
}

fn fmt6(m: &[f64; 6]) -> String {
    m.iter().map(|v| format!("{v:.5}")).collect::<Vec<_>>().join(" ")
}

pub fn save(m: &[f64; 6]) -> std::result::Result<(), String> {
    let six = fmt6(m);
    let nine = format!("{six} 0 0 1");

    // /etc/default/pocketchip-touch (pocketchip-touch-calibrate and other shell tools)
    let old = std::fs::read_to_string(DEFAULTS).unwrap_or_else(|_| "POCKETCHIP_TOUCH_DEVICE=1c25000.rtp\n".into());
    let mut lines: Vec<String> = old.lines().filter(|l| !l.starts_with("POCKETCHIP_TOUCH_MATRIX=")).map(String::from).collect();
    lines.push(format!("POCKETCHIP_TOUCH_MATRIX='{nine}'"));
    std::fs::write(DEFAULTS, lines.join("\n") + "\n").map_err(|e| format!("{DEFAULTS}: {e}"))?;

    // KMS / libinput via udev
    let rule = format!(
        "# Written by pocketchip-console touch calibration.\nACTION==\"add|change\", KERNEL==\"event*\", ATTRS{{name}}==\"1c25000.rtp\", ENV{{LIBINPUT_CALIBRATION_MATRIX}}=\"{six}\"\n"
    );
    std::fs::write(UDEV, rule).map_err(|e| format!("{UDEV}: {e}"))?;
    let _ = run("udevadm", &["control", "--reload"]);
    let _ = run("udevadm", &["trigger", "--subsystem-match=input", "--action=change"]);
    Ok(())
}
