//! Built-in terminal: a login shell of the PocketCHIP user on a pseudo
//! terminal, parsed by `vt100` and drawn by the console in its own style.

use std::ffi::CString;
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub const SCROLLBACK: usize = 500;

pub struct Term {
    master: OwnedFd,
    pub pid: libc::pid_t,
    pub parser: vt100::Parser,
    pub rows: u16,
    pub cols: u16,
    /// lines scrolled back into the history (0 = live view)
    pub view_offset: usize,
}

/// One run of equally styled characters in a terminal row.
pub struct Seg {
    pub row: u16,
    pub col: u16,
    pub text: String,
    pub inverse: bool,
    pub bold: bool,
}

struct User {
    uid: libc::uid_t,
    gid: libc::gid_t,
    groups: Vec<libc::gid_t>,
    name: String,
    home: String,
    shell: String,
}

fn lookup_user(name: &str) -> Option<User> {
    let cname = CString::new(name).ok()?;
    unsafe {
        let pw = libc::getpwnam(cname.as_ptr());
        if pw.is_null() {
            return None;
        }
        let pw = &*pw;
        let s = |p: *const libc::c_char| std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
        let mut groups = vec![0 as libc::gid_t; 64];
        let mut n: libc::c_int = groups.len() as libc::c_int;
        if libc::getgrouplist(cname.as_ptr(), pw.pw_gid, groups.as_mut_ptr(), &mut n) < 0 {
            n = 0;
        }
        groups.truncate(n.max(0) as usize);
        Some(User { uid: pw.pw_uid, gid: pw.pw_gid, groups, name: s(pw.pw_name), home: s(pw.pw_dir), shell: s(pw.pw_shell) })
    }
}

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v.max(1)).unwrap()
}

impl Term {
    /// Start the user's login shell. Runs as root, drops to the user.
    pub fn spawn(rows: u16, cols: u16) -> Result<Term, String> {
        let user_name = std::env::var("POCKETCHIP_USER").unwrap_or_else(|_| "chip".into());
        let u = lookup_user(&user_name).ok_or_else(|| format!("BENUTZER {user_name} FEHLT"))?;
        let shell = if u.shell.is_empty() { "/bin/sh".to_string() } else { u.shell.clone() };
        let argv0 = format!("-{}", shell.rsplit('/').next().unwrap_or("sh"));

        // everything the child needs is prepared before fork (threads!)
        let c_shell = CString::new(shell.clone()).map_err(|e| e.to_string())?;
        let c_argv0 = CString::new(argv0).map_err(|e| e.to_string())?;
        let argv = [c_argv0.as_ptr(), std::ptr::null()];
        let env: Vec<CString> = [
            ("TERM", "xterm-256color".to_string()),
            ("COLORTERM", String::new()),
            ("HOME", u.home.clone()),
            ("USER", u.name.clone()),
            ("LOGNAME", u.name.clone()),
            ("SHELL", shell.clone()),
            ("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/local/games:/usr/games".to_string()),
            ("LANG", "C.UTF-8".to_string()),
        ]
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| CString::new(format!("{k}={v}")).unwrap())
        .collect();
        let mut envp: Vec<*const libc::c_char> = env.iter().map(|c| c.as_ptr()).collect();
        envp.push(std::ptr::null());
        let c_home = CString::new(u.home.clone()).map_err(|e| e.to_string())?;

        let mut ws = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
        let (mut master, mut slave) = (-1, -1);
        if unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &mut ws) } != 0 {
            return Err("OPENPTY FEHLGESCHLAGEN".into());
        }
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            unsafe {
                libc::close(master);
                libc::close(slave);
            }
            return Err("FORK FEHLGESCHLAGEN".into());
        }
        if pid == 0 {
            // child: only async-signal-safe calls from here on
            unsafe {
                libc::close(master);
                if libc::login_tty(slave) != 0 {
                    libc::_exit(126);
                }
                // undo the console's blocked signal mask for the shell
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
                // Rust ignores SIGPIPE; ignored signals survive exec
                libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                if libc::setgroups(u.groups.len(), u.groups.as_ptr()) != 0
                    || libc::setgid(u.gid) != 0
                    || libc::setuid(u.uid) != 0
                {
                    libc::_exit(126);
                }
                libc::chdir(c_home.as_ptr());
                libc::execve(c_shell.as_ptr(), argv.as_ptr(), envp.as_ptr());
                libc::_exit(127);
            }
        }
        unsafe { libc::close(slave) };
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        Ok(Term { master, pid, parser: vt100::Parser::new(nz(rows), nz(cols), SCROLLBACK), rows, cols, view_offset: 0 })
    }

    /// A duplicate of the master fd for the reader thread.
    pub fn reader_fd(&self) -> Option<OwnedFd> {
        let fd = unsafe { libc::dup(self.master.as_raw_fd()) };
        if fd < 0 { None } else { Some(unsafe { OwnedFd::from_raw_fd(fd) }) }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        // back to the live view when typing
        if self.view_offset != 0 {
            self.view_offset = 0;
            self.parser.screen_mut().set_scrollback(0);
        }
        let mut off = 0;
        while off < bytes.len() {
            let n = unsafe { libc::write(self.master.as_raw_fd(), bytes[off..].as_ptr() as *const libc::c_void, bytes.len() - off) };
            if n <= 0 {
                break;
            }
            off += n as usize;
        }
    }

    /// New grid size: tell the shell (SIGWINCH via TIOCSWINSZ) and the parser.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        let ws = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
        unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ as _, &ws) };
        self.parser.screen_mut().set_size(nz(rows), nz(cols));
        self.rows = rows;
        self.cols = cols;
    }

    pub fn scroll(&mut self, lines: isize) {
        let want = (self.view_offset as isize + lines).clamp(0, SCROLLBACK as isize) as usize;
        self.parser.screen_mut().set_scrollback(want);
        self.view_offset = self.parser.screen().scrollback();
    }

    /// Is the shell still running? Reaps it when it has exited.
    pub fn alive(&self) -> bool {
        let mut st = 0;
        unsafe { libc::waitpid(self.pid, &mut st, libc::WNOHANG) == 0 }
    }

    /// Visible rows as styled runs; the cursor is drawn inverted.
    pub fn segments(&self) -> Vec<Seg> {
        let screen = self.parser.screen();
        let (crow, ccol) = screen.cursor_position();
        let show_cursor = !screen.hide_cursor() && self.view_offset == 0;
        let mut out = Vec::new();
        for r in 0..self.rows {
            let mut cur: Option<Seg> = None;
            for c in 0..self.cols {
                let Some(cell) = screen.cell(r, c) else { continue };
                if cell.is_wide_continuation() {
                    continue;
                }
                let is_cursor = show_cursor && r == crow && c == ccol;
                // monochrome: inverse video or any background colour is a highlight
                let highlight = cell.inverse() || cell.bgcolor() != vt100::Color::Default;
                let inverse = highlight ^ is_cursor;
                let bold = cell.bold();
                let ch = if cell.has_contents() { cell.contents() } else { " " };
                match cur.as_mut() {
                    Some(s) if s.inverse == inverse && s.bold == bold && !cell.is_wide() => s.text.push_str(ch),
                    _ => {
                        if let Some(s) = cur.take() {
                            out.push(s);
                        }
                        cur = Some(Seg { row: r, col: c, text: ch.to_string(), inverse, bold });
                    }
                }
                if cell.is_wide() {
                    if let Some(s) = cur.take() {
                        out.push(s);
                    }
                }
            }
            if let Some(s) = cur.take() {
                out.push(s);
            }
        }
        // drop runs that draw nothing
        out.retain(|s| s.inverse || s.text.chars().any(|c| c != ' '));
        out
    }

    /// Bytes for a key press (text after the PocketCHIP Fn mapping).
    pub fn encode_key(&self, text: &str, ctrl: bool) -> Vec<u8> {
        use slint::platform::Key;
        let app = self.parser.screen().application_cursor();
        let arrow = |c: u8| if app { vec![0x1b, b'O', c] } else { vec![0x1b, b'[', c] };
        let mut chars = text.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            return text.as_bytes().to_vec();
        };
        let k = |key: Key| char::from(key) == ch;
        if k(Key::UpArrow) { return arrow(b'A'); }
        if k(Key::DownArrow) { return arrow(b'B'); }
        if k(Key::RightArrow) { return arrow(b'C'); }
        if k(Key::LeftArrow) { return arrow(b'D'); }
        if k(Key::Home) { return b"\x1b[H".to_vec(); }
        if k(Key::End) { return b"\x1b[F".to_vec(); }
        if k(Key::PageUp) { return b"\x1b[5~".to_vec(); }
        if k(Key::PageDown) { return b"\x1b[6~".to_vec(); }
        if k(Key::Delete) { return b"\x1b[3~".to_vec(); }
        if k(Key::Insert) { return b"\x1b[2~".to_vec(); }
        if k(Key::Return) || ch == '\n' { return b"\r".to_vec(); }
        if k(Key::Backspace) { return vec![0x7f]; }
        if k(Key::Tab) { return b"\t".to_vec(); }
        if k(Key::Escape) { return vec![0x1b]; }
        for (i, f) in [Key::F1, Key::F2, Key::F3, Key::F4].iter().enumerate() {
            if k(*f) { return vec![0x1b, b'O', b'P' + i as u8]; }
        }
        // other private-use key codes (shift, ctrl, ...) produce nothing
        if ('\u{f700}'..='\u{f8ff}').contains(&ch) || ch == '\u{10}' || ch == '\u{11}' || ch == '\u{12}' {
            return Vec::new();
        }
        if ctrl {
            let l = ch.to_ascii_lowercase();
            if l.is_ascii_lowercase() {
                return vec![(l as u8) - b'a' + 1];
            }
            match ch {
                ' ' | '@' | '2' => return vec![0],
                '[' => return vec![0x1b],
                '\\' => return vec![0x1c],
                ']' => return vec![0x1d],
                '^' | '6' => return vec![0x1e],
                '_' | '-' => return vec![0x1f],
                _ => {}
            }
        }
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf).as_bytes().to_vec()
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.pid, libc::SIGHUP);
            libc::kill(self.pid, libc::SIGHUP);
            let mut st = 0;
            libc::waitpid(self.pid, &mut st, libc::WNOHANG);
        }
    }
}

/// Read the pty until EOF and hand the bytes to `cb` (runs on its own thread).
pub fn spawn_reader(fd: OwnedFd, cb: impl Fn(Option<Vec<u8>>) + Send + 'static) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 8192];
        loop {
            let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n <= 0 {
                cb(None);
                break;
            }
            cb(Some(buf[..n as usize].to_vec()));
        }
    });
}
