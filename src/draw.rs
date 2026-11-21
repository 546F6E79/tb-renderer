//! Drawing primitives on an XRGB8888 (in-memory B,G,R,X) Touch Bar frame.
//! Controls, media, volume and keyboard-backlight icons are procedural; text and brightness use the atlas.

pub const BG: [u8; 4] = [0x00, 0x00, 0x00, 0xff];
pub const SEG: [u8; 4] = [0x2e, 0x2c, 0x2c, 0xff]; // #2c2c2e
pub const SEG_HOVER: [u8; 4] = [0x3c, 0x3a, 0x3a, 0xff]; // #3a3a3c
pub const SEG_PRESSED: [u8; 4] = [0x4a, 0x48, 0x48, 0xff]; // #48484a
pub const FG: [u8; 4] = [0xf7, 0xf5, 0xf5, 0xff]; // #f5f5f7
pub const FG_DIM: [u8; 4] = [0x73, 0x6e, 0x6e, 0xff]; // #6e6e73
pub const FG_TRACK: [u8; 4] = [0xd6, 0xd1, 0xd1, 0xff]; // #d1d1d6

pub struct Canvas<'a> {
    pub w: u32,
    pub h: u32,
    pub stride: u32,
    pub buf: &'a mut [u8],
}

impl<'a> Canvas<'a> {
    pub fn px(&mut self, x: u32, y: u32, c: [u8; 4]) {
        if x >= self.w || y >= self.h {
            return;
        }
        let o = (y * self.stride + x * 4) as usize;
        self.buf[o..o + 4].copy_from_slice(&c);
    }

    pub fn fill(&mut self, c: [u8; 4]) {
        for y in 0..self.h {
            for x in 0..self.w {
                self.px(x, y, c);
            }
        }
    }

    pub fn rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                self.px(x, y, c);
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, c: [u8; 4], alpha: f64) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || alpha <= 0.0 {
            return;
        }
        let a = alpha.min(1.0);
        let o = (y as u32 * self.stride + x as u32 * 4) as usize;
        for i in 0..3 {
            self.buf[o + i] = (self.buf[o + i] as f64 * (1.0 - a) + c[i] as f64 * a).round() as u8;
        }
    }

    fn rounded_distance(
        &self,
        px: f64,
        py: f64,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        r: f64,
    ) -> f64 {
        let qx = (px - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - r);
        let qy = (py - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - r);
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        outside + qx.max(qy).min(0.0) - r
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, width: f64, c: [u8; 4]) {
        let pad = width / 2.0 + 1.0;
        let min_x = (x0.min(x1) - pad).floor() as i32;
        let max_x = (x0.max(x1) + pad).ceil() as i32;
        let min_y = (y0.min(y1) - pad).floor() as i32;
        let max_y = (y0.max(y1) + pad).ceil() as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len2 = vx * vx + vy * vy;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    ((px - x0) * vx + (py - y0) * vy) / len2
                }
                .clamp(0.0, 1.0);
                let distance = ((px - x0 - t * vx).powi(2) + (py - y0 - t * vy).powi(2)).sqrt();
                self.blend(x, y, c, width / 2.0 + 0.5 - distance);
            }
        }
    }

    fn rounded_stroke(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        r: f64,
        width: f64,
        c: [u8; 4],
    ) {
        for y in (y0 - width).floor() as i32..=(y1 + width).ceil() as i32 {
            for x in (x0 - width).floor() as i32..=(x1 + width).ceil() as i32 {
                let d = self.rounded_distance(x as f64 + 0.5, y as f64 + 0.5, x0, y0, x1, y1, r);
                self.blend(x, y, c, width / 2.0 + 0.5 - d.abs());
            }
        }
    }

    fn rounded_fill(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, r: f64, c: [u8; 4]) {
        for y in y0.floor() as i32..y1.ceil() as i32 {
            for x in x0.floor() as i32..x1.ceil() as i32 {
                let d = self.rounded_distance(x as f64 + 0.5, y as f64 + 0.5, x0, y0, x1, y1, r);
                self.blend(x, y, c, 0.5 - d);
            }
        }
    }

    /// Fill a convex polygon with one-pixel antialiasing at its edges.
    fn polygon(&mut self, points: &[(f64, f64)], c: [u8; 4]) {
        if points.len() < 3 {
            return;
        }
        let min_x = points
            .iter()
            .map(|p| p.0)
            .fold(f64::INFINITY, f64::min)
            .floor() as i32
            - 1;
        let max_x = points
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as i32
            + 1;
        let min_y = points
            .iter()
            .map(|p| p.1)
            .fold(f64::INFINITY, f64::min)
            .floor() as i32
            - 1;
        let max_y = points
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as i32
            + 1;
        let area: f64 = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
            .sum();
        let orientation = area.signum();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let mut nearest = f64::INFINITY;
                for (a, b) in points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .take(points.len())
                {
                    let ex = b.0 - a.0;
                    let ey = b.1 - a.1;
                    let distance = (ex * (py - a.1) - ey * (px - a.0)) * orientation
                        / (ex * ex + ey * ey).sqrt();
                    nearest = nearest.min(distance);
                }
                self.blend(x, y, c, nearest + 0.5);
            }
        }
    }

    /// Rounded macOS-style segment with antialiased corners.
    pub fn seg_bg(&mut self, x0: u32, x1: u32, hover: bool, pressed: bool) {
        let c = if pressed {
            SEG_PRESSED
        } else if hover {
            SEG_HOVER
        } else {
            SEG
        };
        self.rounded_fill(x0 as f64, 2.0, x1 as f64, self.h as f64 - 2.0, 10.0, c);
    }

    /// Apple's keyboard-backlight metaphor: a small keyboard deck with light rays.
    pub fn keyboard_backlight(&mut self, cx: u32, cy: u32, brighter: bool, c: [u8; 4]) {
        let s = if brighter { 1.0 } else { 0.76 };
        let cx = cx as f64;
        let cy = cy as f64;
        let point = |x: f64, y: f64| (cx + x * s, cy + y * s);
        let ray = |cv: &mut Self, x0: f64, y0: f64, x1: f64, y1: f64| {
            let (ax, ay) = point(x0, y0);
            let (bx, by) = point(x1, y1);
            cv.line(ax, ay, bx, by, 2.0 * s, c);
        };
        ray(self, 0.0, -14.0, 0.0, -7.0);
        ray(self, -7.0, -12.0, -5.5, -5.5);
        ray(self, 7.0, -12.0, 5.5, -5.5);
        ray(self, -13.0, -8.0, -9.5, -2.5);
        ray(self, 13.0, -8.0, 9.5, -2.5);

        let (x0, y0) = point(-16.0, 3.0);
        let (x1, y1) = point(16.0, 12.0);
        self.rounded_stroke(x0, y0, x1, y1, 2.0 * s, 1.8 * s, c);
        for x in [-5.3, 5.3] {
            let (ax, ay) = point(x, 4.5);
            let (bx, by) = point(x, 10.5);
            self.line(ax, ay, bx, by, 1.2 * s, c);
        }
    }

    pub fn media_backward(&mut self, cx: u32, cy: u32, c: [u8; 4]) {
        let cx = cx as f64;
        let cy = cy as f64;
        for offset in [-5.5, 5.5] {
            self.polygon(
                &[
                    (cx + offset + 5.0, cy - 9.0),
                    (cx + offset - 5.0, cy),
                    (cx + offset + 5.0, cy + 9.0),
                ],
                c,
            );
        }
    }

    pub fn media_forward(&mut self, cx: u32, cy: u32, c: [u8; 4]) {
        let cx = cx as f64;
        let cy = cy as f64;
        for offset in [-5.5, 5.5] {
            self.polygon(
                &[
                    (cx + offset - 5.0, cy - 9.0),
                    (cx + offset + 5.0, cy),
                    (cx + offset - 5.0, cy + 9.0),
                ],
                c,
            );
        }
    }

    pub fn media_play(&mut self, cx: u32, cy: u32, c: [u8; 4]) {
        let cx = cx as f64;
        let cy = cy as f64;
        self.polygon(
            &[(cx - 7.0, cy - 11.0), (cx + 9.0, cy), (cx - 7.0, cy + 11.0)],
            c,
        );
    }

    pub fn media_pause(&mut self, cx: u32, cy: u32, c: [u8; 4]) {
        let cx = cx as f64;
        let cy = cy as f64;
        self.rounded_fill(cx - 8.0, cy - 10.0, cx - 3.0, cy + 10.0, 1.2, c);
        self.rounded_fill(cx + 3.0, cy - 10.0, cx + 8.0, cy + 10.0, 1.2, c);
    }

    fn sound_wave(&mut self, cx: f64, cy: f64, radius: f64, c: [u8; 4]) {
        const SEGMENTS: u32 = 24;
        let angle = |i: u32| -0.85 + 1.7 * i as f64 / SEGMENTS as f64;
        for i in 0..SEGMENTS {
            let a = angle(i);
            let b = angle(i + 1);
            self.line(
                cx + radius * a.cos(),
                cy + radius * a.sin(),
                cx + radius * b.cos(),
                cy + radius * b.sin(),
                1.6,
                c,
            );
        }
    }

    /// Speaker with one wave for volume down and two for volume up.
    pub fn volume(&mut self, cx: u32, cy: u32, louder: bool, c: [u8; 4]) {
        let cx = cx as f64;
        let cy = cy as f64;
        self.rounded_fill(cx - 13.0, cy - 4.0, cx - 8.0, cy + 4.0, 1.0, c);
        self.polygon(
            &[
                (cx - 8.0, cy - 4.0),
                (cx - 2.0, cy - 10.0),
                (cx - 2.0, cy + 10.0),
                (cx - 8.0, cy + 4.0),
            ],
            c,
        );
        self.sound_wave(cx - 2.0, cy, 7.0, c);
        if louder {
            self.sound_wave(cx - 2.0, cy, 12.0, c);
        }
    }

    /// Axis-aligned play triangle pointing right, centered in (x0..x1) x (y0..y1).
    pub fn triangle_right(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
        let w = x1.saturating_sub(x0);
        let h = y1.saturating_sub(y0);
        if w == 0 || h == 0 {
            return;
        }
        for row in 0..h {
            let half = (row as f64 + 0.5 - h as f64 / 2.0).abs();
            let span = (half * w as f64 / (h as f64 / 2.0)).round() as u32;
            let y = y0 + row;
            for x in x0..(x1 - span.min(w)) {
                self.px(x, y, c);
            }
        }
    }

    pub fn triangle_left(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
        let w = x1.saturating_sub(x0);
        let h = y1.saturating_sub(y0);
        if w == 0 || h == 0 {
            return;
        }
        for row in 0..h {
            let half = (row as f64 + 0.5 - h as f64 / 2.0).abs();
            let span = (half * w as f64 / (h as f64 / 2.0)).round() as u32;
            let y = y0 + row;
            for x in (x0 + span.min(w))..x1 {
                self.px(x, y, c);
            }
        }
    }

    pub fn circle(&mut self, cx: u32, cy: u32, r: u32, c: [u8; 4]) {
        for y in cy.saturating_sub(r)..=cy + r {
            for x in cx.saturating_sub(r)..=cx + r {
                let dx = x as i64 - cx as i64;
                let dy = y as i64 - cy as i64;
                if dx * dx + dy * dy <= r as i64 * r as i64 {
                    self.px(x, y, c);
                }
            }
        }
    }

    pub fn bar(&mut self, x0: u32, y0: u32, w: u32, h: u32, c: [u8; 4]) {
        self.rect(x0, y0, x0 + w, y0 + h, c);
    }

    /// Tiny 5x7 bitmap digit renderer (for F1..F12 labels).
    pub fn glyph(&mut self, ch: char, x0: u32, y0: u32, scale: u32, c: [u8; 4]) {
        let rows: [u8; 7] = match ch.to_ascii_uppercase() {
            '0' => [
                0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
            ],
            '1' => [
                0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
            ],
            '2' => [
                0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
            ],
            '3' => [
                0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
            ],
            '4' => [
                0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
            ],
            '5' => [
                0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
            ],
            '6' => [
                0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
            ],
            '7' => [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
            ],
            '8' => [
                0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
            ],
            '9' => [
                0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
            ],
            'F' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
            ],
            'E' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
            ],
            'S' => [
                0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
            ],
            'C' => [
                0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
            ],
            _ => return,
        };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..5u32 {
                if (bits >> (4 - rx)) & 1 == 1 {
                    self.rect(
                        x0 + rx * scale,
                        y0 + ry as u32 * scale,
                        x0 + (rx + 1) * scale,
                        y0 + (ry as u32 + 1) * scale,
                        c,
                    );
                }
            }
        }
    }

    /// Ring (annulus) centered at (cx,cy): outer radius ro, inner ri.
    pub fn ring(&mut self, cx: u32, cy: u32, ro: u32, ri: u32, c: [u8; 4]) {
        for y in cy.saturating_sub(ro)..=cy + ro {
            for x in cx.saturating_sub(ro)..=cx + ro {
                let dx = x as i64 - cx as i64;
                let dy = y as i64 - cy as i64;
                let d2 = dx * dx + dy * dy;
                if d2 <= ro as i64 * ro as i64 && d2 >= ri as i64 * ri as i64 {
                    self.px(x, y, c);
                }
            }
        }
    }

    /// Arc of a ring: only the right-opening side (for speaker waves).
    pub fn arc_right(&mut self, cx: u32, cy: u32, r: u32, c: [u8; 4]) {
        for y in cy.saturating_sub(r)..=cy + r {
            for x in cx..=cx + r {
                let dx = x as i64 - cx as i64;
                let dy = y as i64 - cy as i64;
                let d2 = dx * dx + dy * dy;
                let target = r as i64 * r as i64;
                if d2 <= target && d2 >= (r - 2) as i64 * (r - 2) as i64 && dx >= 0 {
                    self.px(x, y, c);
                }
            }
        }
    }

    /// Глиф из атласа: 8-битная альфа, blended. pen_x — перо, baseline — базовая линия.
    pub fn glyph_alpha(
        &mut self,
        alpha: &[u8],
        w: u32,
        h: u32,
        left: i32,
        top: i32,
        pen_x: u32,
        baseline: i32,
        c: [u8; 4],
    ) {
        let x0 = pen_x as i64 + left as i64;
        let y0 = baseline as i64 - top as i64;
        for row in 0..h {
            for col in 0..w {
                let a = alpha[(row * w + col) as usize] as u32;
                if a == 0 {
                    continue;
                }
                let px = x0 + col as i64;
                let py = y0 + row as i64;
                if px < 0 || py < 0 || px >= self.w as i64 || py >= self.h as i64 {
                    continue;
                }
                let o = (py as u32 * self.stride + px as u32 * 4) as usize;
                for i in 0..3 {
                    self.buf[o + i] =
                        ((self.buf[o + i] as u32 * (255 - a) + c[i] as u32 * a) / 255) as u8;
                }
            }
        }
    }

    pub fn text(&mut self, s: &str, x0: u32, y0: u32, scale: u32, c: [u8; 4]) -> u32 {
        let mut x = x0;
        for ch in s.chars() {
            if ch == ' ' {
                x += 3 * scale;
                continue;
            }
            self.glyph(ch, x, y0, scale, c);
            x += 6 * scale;
        }
        x - x0
    }
}
