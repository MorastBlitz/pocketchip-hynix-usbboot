//! Debug control file: /run/pocketchip-console/cmd (root only).
//! Lines: `snap PATH`, `key up|down|left|right|enter|esc|<char>`, `tap X Y`.

use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, SharedString};

pub const DIR: &str = "/run/pocketchip-console";

fn key_text(k: &str) -> SharedString {
    match k {
        "up" => Key::UpArrow.into(),
        "down" => Key::DownArrow.into(),
        "left" => Key::LeftArrow.into(),
        "right" => Key::RightArrow.into(),
        "enter" => Key::Return.into(),
        "esc" => Key::Escape.into(),
        "tab" => Key::Tab.into(),
        "bs" => Key::Backspace.into(),
        "space" => " ".into(),
        other => other.into(),
    }
}

fn snap(ui: &crate::AppWindow, path: &str) {
    let Ok(buf) = ui.window().take_snapshot() else {
        eprintln!("snapshot failed");
        return;
    };
    let Ok(f) = std::fs::File::create(path) else { return };
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), buf.width(), buf.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        let _ = w.write_image_data(buf.as_bytes());
    }
}

pub fn poll(ui: &crate::AppWindow) {
    let path = format!("{DIR}/cmd");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let _ = std::fs::remove_file(&path);
    let w = ui.window();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.as_slice() {
            ["snap", p] => snap(ui, p),
            ["key", k] => {
                let t = key_text(k);
                w.dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
                w.dispatch_event(WindowEvent::KeyReleased { text: t });
            }
            ["type", rest @ ..] => {
                for c in rest.join(" ").chars() {
                    let t: SharedString = c.to_string().into();
                    w.dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
                    w.dispatch_event(WindowEvent::KeyReleased { text: t });
                }
            }
            ["tap", x, y] => {
                let p = LogicalPosition::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                w.dispatch_event(WindowEvent::PointerMoved { position: p });
                w.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
                w.dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
                w.dispatch_event(WindowEvent::PointerExited);
            }
            ["raw", x, y] => {
                let (x, y) = (x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                crate::with_app(move |a| a.on_input(crate::hw::InputEvent::TouchRaw(x, y)))
            }
            ["activity"] => crate::with_app(|a| a.on_input(crate::hw::InputEvent::Activity)),
            ["power"] => crate::with_app(|a| a.on_input(crate::hw::InputEvent::PowerKey)),
            _ => {}
        }
    }
}
