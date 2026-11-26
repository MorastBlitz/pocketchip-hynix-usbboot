//! Headphone output of the sun4i codec (3.5 mm jack) through amixer.
//! The PocketCHIP has no built-in speaker or microphone.

use crate::util::run_timeout;
use std::time::Duration;

const CARD: &str = "0";

fn amixer(args: &[&str]) -> bool {
    let mut a = vec!["-q", "-c", CARD, "sset"];
    a.extend_from_slice(args);
    run_timeout("amixer", &a, Duration::from_secs(3)).0
}

/// Route DAC -> headphone amplifier and set volume (0..100 %) and mute.
pub fn apply(volume: i64, muted: bool) -> bool {
    let ok = amixer(&["Power Amplifier DAC", "on"]);
    // "Power Amplifier Mute" is a playback switch: on = sound, off = muted
    amixer(&["Power Amplifier Mute", if muted || volume == 0 { "off" } else { "on" }]);
    amixer(&["Power Amplifier", &format!("{}%", volume.clamp(0, 100))]);
    ok
}

/// Short 440 Hz test tone in the background.
pub fn test_tone() {
    std::thread::spawn(|| {
        let _ = run_timeout(
            "speaker-test",
            &["-D", "plughw:0,0", "-t", "sine", "-f", "440", "-c", "2", "-l", "1"],
            Duration::from_millis(1500),
        );
    });
}
