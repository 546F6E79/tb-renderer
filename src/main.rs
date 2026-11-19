//! tb-renderer — custom T1Bridge Touch Bar renderer.
//! Full row (Esc, brightness, volume, live MPRIS media, kbd backlight) + Fn→F1..F12.
//! Protocol: renderer-selection v1 over /run/t1bridge/touchbar.sock (SOCK_SEQPACKET).

mod atlas;
mod conn;
mod draw;
mod frame;
mod layout;
mod media;
mod wire;

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use conn::Conn;
use draw::Canvas;
use frame::Frame;
use layout::{MediaState, Seg};

const PROBE_SETS: [u64; 4] = [
    wire::FEATURE_MEMFD | wire::FEATURE_INPUT | wire::FEATURE_TAPKEYS | wire::FEATURE_DISPLAY_BRIGHTNESS | wire::FEATURE_KEYBOARD_BACKLIGHT,
    wire::FEATURE_MEMFD | wire::FEATURE_INPUT | wire::FEATURE_TAPKEYS | wire::FEATURE_DISPLAY_BRIGHTNESS,
    wire::FEATURE_MEMFD | wire::FEATURE_INPUT | wire::FEATURE_TAPKEYS | wire::FEATURE_KEYBOARD_BACKLIGHT,
    wire::FEATURE_MEMFD | wire::FEATURE_INPUT | wire::FEATURE_TAPKEYS,
];

const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_INTERVAL: Duration = Duration::from_millis(90);

fn log(msg: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "tb-renderer: {msg}");
}

#[derive(Clone, Copy, PartialEq)]
struct TouchState {
    tip: bool,
    in_range: bool,
    x: u32,
    press_seg: Option<Seg>,
}

struct App {
    pressed_seg: Option<Seg>,
    width: u32,
    height: u32,
    frame: Option<Frame>,
    fn_held: bool,
    media: MediaState,
    touches: HashMap<u8, TouchState>,
    hover_seg: Option<Seg>,
    in_flight: bool,
    frame_id: u64,
    req_id: u32,
    dirty: bool,
    /// contact id -> (seg, next repeat instant) for hold-repeatable segments
    repeats: HashMap<u8, (Seg, Instant)>,
}

impl App {
    fn new() -> App {
        App {
            pressed_seg: None,
            width: 0,
            height: 0,
            frame: None,
            fn_held: false,
            media: MediaState::NoPlayer,
            touches: HashMap::new(),
            hover_seg: None,
            in_flight: false,
            frame_id: 1,
            req_id: 1,
            dirty: true,
            repeats: HashMap::new(),
        }
    }

    fn next_req(&mut self) -> u32 {
        loop {
            self.req_id = self.req_id.wrapping_add(1);
            if self.req_id != 0 {
                return self.req_id;
            }
        }
    }

    fn next_frame_id(&mut self) -> u64 {
        loop {
            self.frame_id = self.frame_id.wrapping_add(1);
            if self.frame_id != 0 {
                return self.frame_id;
            }
        }
    }

    fn do_action(&mut self, conn: &Conn, seg: Seg) {
        match seg {
            Seg::Esc => self.send_tap(conn, wire::KEY_ESC),
            Seg::F(n) => {
                if let Some(k) = Some(wire::key_f(n)).filter(|k| *k != 0) {
                    self.send_tap(conn, k);
                }
            }
            Seg::BrightDown => {
                let r = self.next_req();
                let _ = conn.send(&wire::step_display(false, r));
            }
            Seg::BrightUp => {
                let r = self.next_req();
                let _ = conn.send(&wire::step_display(true, r));
            }
            Seg::KbdDown => {
                let r = self.next_req();
                let _ = conn.send(&wire::step_keyboard(false, r));
            }
            Seg::KbdUp => {
                let r = self.next_req();
                let _ = conn.send(&wire::step_keyboard(true, r));
            }
            Seg::VolDown => media::volume(false),
            Seg::VolUp => media::volume(true),
            Seg::Prev => media::player_cmd("previous"),
            Seg::Next => media::player_cmd("next"),
            Seg::PlayPause => {
                media::player_cmd("play-pause");
                // мгновенный отклик без ожидания поллера (поллер скорректирует)
                self.media = match self.media {
                    MediaState::Playing => MediaState::Paused,
                    MediaState::Paused => MediaState::Playing,
                    MediaState::NoPlayer => MediaState::NoPlayer,
                };
                self.dirty = true;
            }
        }
    }

    fn send_tap(&mut self, conn: &Conn, key: u16) {
        let r = self.next_req();
        if let Some(msg) = wire::tap_keys(&[key], r) {
            let _ = conn.send(&msg);
        }
    }

    fn handle_input(&mut self, conn: &Conn, inp: &wire::InputFrame) {
        let row = layout::build(self.width, self.fn_held);
        if inp.fn_pressed != self.fn_held {
            self.fn_held = inp.fn_pressed;
            self.dirty = true;
            log(if self.fn_held { "Fn held -> F-row" } else { "Fn released -> main row" });
        }

        let mut fire: Vec<Seg> = Vec::new();
        let mut repeat_start: Vec<(u8, Seg)> = Vec::new();
        let mut repeat_stop: Vec<u8> = Vec::new();

        for c in &inp.contacts {
            let st = self.touches.entry(c.id).or_insert(TouchState {
                tip: false,
                in_range: false,
                x: c.x,
                press_seg: None,
            });
            let was_tip = st.tip;
            st.in_range = c.in_range;
            st.x = c.x;
            st.tip = c.tip;
            if c.tip && !was_tip {
                st.press_seg = row.seg_at(c.x);
                if let Some(seg) = st.press_seg {
                    if layout::repeatable(seg) {
                        fire.push(seg); // immediate action
                        repeat_start.push((c.id, seg));
                    }
                }
            }
            if !c.tip && was_tip {
                if let Some(seg) = st.press_seg {
                    if !layout::repeatable(seg) {
                        fire.push(seg); // tap on release
                    }
                }
                st.press_seg = None;
                repeat_stop.push(c.id);
            }
        }

        // contacts that vanished without a lift frame => treat as released
        let seen: Vec<u8> = inp.contacts.iter().map(|c| c.id).collect();
        let vanished: Vec<u8> = self
            .touches
            .iter()
            .filter(|(id, st)| st.tip && !seen.contains(id)
                )
            .map(|(id, _)| *id)
            .collect();
        for id in vanished {
            if let Some(st) = self.touches.get_mut(&id) {
                st.tip = false;
                if let Some(seg) = st.press_seg {
                    if !layout::repeatable(seg) {
                        fire.push(seg);
                    }
                }
                st.press_seg = None;
            }
            repeat_stop.push(id);
        }

        for seg in fire {
            self.do_action(conn, seg);
        }
        for id in repeat_stop {
            self.repeats.remove(&id);
        }
        for (id, seg) in repeat_start {
            self.repeats.insert(id, (seg, Instant::now() + REPEAT_DELAY));
        }

        // отдача: сегмент под первым касанием (tip=1)
        let pressed = inp
            .contacts
            .iter()
            .find(|c| c.tip)
            .and_then(|c| row.seg_at(c.x));
        if pressed != self.pressed_seg {
            self.pressed_seg = pressed;
            self.dirty = true;
        }

        // hover: first tip=0,in_range=1 contact defines the hovered segment
        let hover = inp
            .contacts
            .iter()
            .find(|c| !c.tip && c.in_range)
            .and_then(|c| row.seg_at(c.x));
        if hover != self.hover_seg {
            self.hover_seg = hover;
            self.dirty = true;
        }
    }

    fn tick_repeats(&mut self, conn: &Conn) {
        let now = Instant::now();
        let due: Vec<(u8, Seg)> = self
            .repeats
            .iter()
            .filter(|(_, (_, t))| *t <= now)
            .map(|(id, (seg, _))| (*id, *seg))
            .collect();
        for (id, seg) in due {
            self.do_action(conn, seg);
            if let Some(e) = self.repeats.get_mut(&id) {
                e.1 = now + REPEAT_INTERVAL;
            }
        }
    }

    fn render(&mut self) {
        // (иконки регулировок используют общий бейдж −/+)
        let Some(f) = self.frame.as_mut() else { return };
        let mut cv = Canvas { w: self.width, h: self.height, stride: f.stride, buf: f.pixels() };
        cv.fill(draw::BG);
        let row = layout::build(self.width, self.fn_held);
        let playing = self.media == MediaState::Playing;
        let has_player = self.media != MediaState::NoPlayer;
        for (seg, x0, x1) in &row.segs {
            let hover = self.hover_seg == Some(*seg);
            let pressed = self.pressed_seg == Some(*seg);
            cv.seg_bg(*x0, *x1, hover, pressed);
            let cy0 = 12u32;
            let cy1 = self.height - 12;
            let mid = (x0 + x1) / 2;
            let w = x1 - x0;
            let icon: [u8; 4] = match seg {
                Seg::Prev | Seg::PlayPause | Seg::Next if !has_player => draw::FG_DIM,
                _ => draw::FG,
            };
            match seg {
                Seg::Esc => {
                    draw_str(&mut cv, "esc", mid, icon);
                }
                Seg::BrightDown | Seg::BrightUp => {
                    // пара как в macOS: контурное солнце = вниз, залитое = вверх
                    let ch = if *seg == Seg::BrightUp { '\u{f185}' } else { '\u{f186}' };
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == ch) {
                        draw_sym(&mut cv, mid, g, icon);
                    }
                }
                Seg::KbdDown | Seg::KbdUp => {
                    // язык метафор как у яркости: контурное солнце = тусклее, залитое = ярче
                    let sun = if *seg == Seg::KbdUp { '\u{f185}' } else { '\u{f186}' };
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == '\u{f11c}') {
                        draw_sym(&mut cv, mid - 14, g, icon);
                    }
                    if let Some(g) = atlas::SYM2.iter().find(|g| g.ch == sun) {
                        draw_sym(&mut cv, mid + 15, g, icon);
                    }
                }
                Seg::VolDown | Seg::VolUp => {
                    let ch = if *seg == Seg::VolUp { '\u{f028}' } else { '\u{f027}' };
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == ch) {
                        draw_sym(&mut cv, mid, g, icon);
                    }
                }
                Seg::Prev => {
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == '\u{f04a}') {
                        draw_sym(&mut cv, mid, g, icon);
                    }
                }
                Seg::Next => {
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == '\u{f04e}') {
                        draw_sym(&mut cv, mid, g, icon);
                    }
                }
                Seg::PlayPause => {
                    let ch = if playing { '\u{f04c}' } else { '\u{f04b}' };
                    let color = if playing && has_player { draw::ACCENT } else { icon };
                    if let Some(g) = atlas::SYM.iter().find(|g| g.ch == ch) {
                        draw_sym(&mut cv, mid, g, color);
                    }
                }
                Seg::F(n) => {
                    let label = format!("F{n}");
                    draw_str(&mut cv, &label, mid, icon);
                }
            }
        }
    }

    fn maybe_submit(&mut self, conn: &Conn) {
        if !self.dirty || self.in_flight || self.frame.is_none() {
            return;
        }
        self.render();
        self.dirty = false;
        let bid = self.frame.as_ref().map(|f| f.buffer_id);
        if let Some(bid) = bid {
            let r = self.next_req();
            let fid = self.next_frame_id();
            if conn.send(&wire::submit_frame(bid, fid, r)).is_ok() {
                self.in_flight = true;
            }
        }
    }
}

/// Символ из SYM-атласа, центрированный в сегменте (по advance и вертикали).
fn draw_sym(cv: &mut Canvas, mid: u32, g: &atlas::Glyph, c: [u8; 4]) {
    let baseline = (cv.h as i32 / 2 + g.top as i32 - g.h as i32 / 2) as i32;
    cv.glyph_alpha(g.alpha, g.w, g.h, g.left, g.top, mid.saturating_sub(g.advance / 2), baseline, c);
}

/// Строка из TEXT-атласа, центрированная по сумме advance.
fn draw_str(cv: &mut Canvas, s: &str, mid: u32, c: [u8; 4]) {
    let glyph = |ch: char| atlas::TEXT.iter().find(|g| g.ch.eq_ignore_ascii_case(&ch));
    let total: u32 = s.chars().filter_map(|ch| glyph(ch).map(|g| g.advance)).sum();
    let mut pen = mid.saturating_sub(total / 2);
    let baseline = 41i32; // согласовано с 30px кеглем
    for ch in s.chars() {
        if let Some(g) = glyph(ch) {
            cv.glyph_alpha(g.alpha, g.w, g.h, g.left, g.top, pen, baseline, c);
            pen += g.advance;
        }
    }
}

fn cv_text_width(s: &str, scale: u32) -> u32 {
    s.chars().count() as u32 * 6 * scale
}

fn handshake(conn: &Conn, req: &mut u32) -> Result<(u32, u32), u64> {
    // returns (width,height) or the unsupported feature set (Error 2)
    for set in PROBE_SETS {
        *req += 1;
        if conn.send(&wire::hello(1, set, *req)).is_err() {
            return Err(set);
        }
        let mut buf = [0u8; 65536];
        // wait for HelloAck or Error
        for _ in 0..200 {
            if conn::wait_readable(conn.fd, 20) {
                match conn.recv(&mut buf) {
                    Ok(Some(n)) => {
                        if let Some(p) = wire::decode(&buf[..n]) {
                            match p.kind {
                                wire::MSG_HELLO_ACK => {
                                    if let Some(a) = wire::parse_hello_ack(&p) {
                                        if a.pixel_format == wire::PIXEL_FORMAT_XRGB8888 {
                                            return Ok((a.width, a.height));
                                        }
                                    }
                                    return Err(set);
                                }
                                wire::MSG_ERROR => return Err(set),
                                _ => {}
                            }
                        }
                    }
                    Err(_) => return Err(set),
                    _ => {}
                }
            }
        }
        return Err(set); // timeout
    }
    unreachable!()
}

fn expect_ack(conn: &Conn, req_id: u32) -> bool {
    let mut buf = [0u8; 65536];
    for _ in 0..200 {
        if !conn::wait_readable(conn.fd, 20) {
            continue;
        }
        if let Ok(Some(n)) = conn.recv(&mut buf) {
            if let Some(p) = wire::decode(&buf[..n]) {
                if p.kind == wire::MSG_ACK && p.req_id == req_id {
                    return true;
                }
                if p.kind == wire::MSG_ERROR {
                    log(&format!("error response: {:?}", p.payload));
                    return false;
                }
            }
        }
    }
    false
}

fn run_session(conn: &Conn, rx: &Receiver<MediaState>) {
    let mut app = App::new();
    let mut req = 0u32;
    match handshake(conn, &mut req) {
        Ok((w, h)) => {
            log(&format!("connected: {w}x{h}"));
            app.width = w;
            app.height = h;
            app.req_id = req;
        }
        Err(set) => {
            log(&format!("handshake failed (features {set:#x})"));
            return;
        }
    }

    let frame = match Frame::create(app.width, app.height, 1) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("memfd failed: {e}"));
            return;
        }
    };
    let reg_req = app.next_req();
    if frame.register(conn, reg_req).is_err() {
        log("register_buffer send failed");
        return;
    }
    if !expect_ack(conn, reg_req) {
        log("register_buffer not acked");
        return;
    }
    app.frame = Some(frame);
    app.dirty = true;

    let mut buf = [0u8; 65536];
    loop {
        // media updates
        loop {
            match rx.try_recv() {
                Ok(m) => {
                    if app.media != m {
                        app.media = m;
                        app.dirty = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        app.tick_repeats(conn);
        if conn::wait_readable(conn.fd, 20) {
            match conn.recv(&mut buf) {
                Ok(Some(n)) => {
                    if let Some(p) = wire::decode(&buf[..n]) {
                        match p.kind {
                            wire::MSG_INPUT_FRAME => {
                                if let Some(inp) = wire::parse_input_frame(&p) {
                                    app.handle_input(conn, &inp);
                                }
                            }
                            wire::MSG_FRAME_RELEASED => {
                                if let Some(fr) = wire::parse_frame_released(&p) {
                                    if fr.frame_id != 0 {
                                        app.in_flight = false;
                                    }
                                }
                            }
                            wire::MSG_ERROR => {
                                log(&format!("service error: {:?}", p.payload));
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    log(&format!("recv error: {e}"));
                    return;
                }
            }
        }
        app.maybe_submit(conn);
    }
}

fn main() {
    log("starting (renderer-selection v1)");
    let (tx, rx) = std::sync::mpsc::channel();
    media::spawn(tx);
    let mut backoff = 50u64;
    loop {
        match Conn::connect(conn::SOCKET_PATH) {
            Ok(conn) => {
                backoff = 50;
                run_session(&conn, &rx);
                log("session ended");
            }
            Err(e) => {
                let _ = e;
            }
        }
        std::thread::sleep(Duration::from_millis(backoff));
        backoff = (backoff * 2).min(2000);
    }
}
