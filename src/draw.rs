//! Drawing primitives on an XRGB8888 (in-memory B,G,R,X) Touch Bar frame.
//! Everything is procedural: no fonts, no assets. Icons are geometric shapes.

pub const BG: [u8; 4] = [0x22, 0x1e, 0x1a, 0xff];      // #1a1e22
pub const SEG: [u8; 4] = [0x34, 0x2e, 0x28, 0xff];     // #282e34
pub const SEG_HOVER: [u8; 4] = [0x4d, 0x44, 0x3a, 0xff]; // #3a444d
pub const SEG_PRESSED: [u8; 4] = [0x66, 0x5a, 0x4d, 0xff]; // #4d5a66
pub const FG: [u8; 4] = [0xfc, 0xfc, 0xfc, 0xff];      // #fcfcfc
pub const FG_DIM: [u8; 4] = [0x8d, 0x8d, 0x8d, 0xff];  // no player
pub const ACCENT: [u8; 4] = [0xf3, 0x99, 0x1d, 0xff];  // #1d99f3 (Breeze blue)

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

    /// Rounded-left/right segment background with a 1px gap look.
    pub fn seg_bg(&mut self, x0: u32, x1: u32, hover: bool, pressed: bool) {
        let c = if pressed { SEG_PRESSED } else if hover { SEG_HOVER } else { SEG };
        let y0 = 4u32;
        let y1 = self.h - 4;
        let r = 8u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = if x < x0 + r {
                    x0 + r - x
                } else if x >= x1.saturating_sub(r) {
                    x - (x1 - 1 - r)
                } else {
                    0
                };
                let dy = if y < y0 + r { y0 + r - y } else if y >= y1 - r { y - (y1 - 1 - r) } else { 0 };
                if dx * dx + dy * dy <= r * r {
                    self.px(x, y, c);
                }
            }
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
            '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
            '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
            '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
            '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
            '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
            '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
            '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
            '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
            '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
            'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
            'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
            'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
            'C' => [0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111],
            _ => return,
        };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..5u32 {
                if (bits >> (4 - rx)) & 1 == 1 {
                    self.rect(x0 + rx * scale, y0 + ry as u32 * scale, x0 + (rx + 1) * scale, y0 + (ry as u32 + 1) * scale, c);
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
    pub fn glyph_alpha(&mut self, alpha: &[u8], w: u32, h: u32, left: i32, top: i32, pen_x: u32, baseline: i32, c: [u8; 4]) {
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
                    self.buf[o + i] = ((self.buf[o + i] as u32 * (255 - a) + c[i] as u32 * a) / 255) as u8;
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
