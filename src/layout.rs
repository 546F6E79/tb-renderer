//! Segment layout + hit-testing for the Touch Bar row.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    Esc,
    BrightDown,
    BrightUp,
    KbdDown,
    KbdUp,
    VolDown,
    VolUp,
    Prev,
    PlayPause,
    Next,
    F(u8), // 1..=12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState {
    NoPlayer,
    Paused,
    Playing,
}

pub struct Row {
    pub segs: Vec<(Seg, u32, u32)>, // (seg, x0, x1) with 8px gaps
}

/// Main row: [esc][sun-][sun+][spk-][spk+][prev][play][next][kbd-][kbd+]
/// Fn row:   [F1][F2]...[F12]
pub fn build(width: u32, fn_held: bool) -> Row {
    let gap = 10u32;
    let kinds: Vec<Seg> = if fn_held {
        (1u8..=12).map(Seg::F).collect()
    } else {
        vec![
            Seg::Esc,
            Seg::BrightDown,
            Seg::BrightUp,
            Seg::VolDown,
            Seg::VolUp,
            Seg::Prev,
            Seg::PlayPause,
            Seg::Next,
            Seg::KbdDown,
            Seg::KbdUp,
        ]
    };
    let n = kinds.len() as u32;
    let inner = width.saturating_sub(gap * (n + 1));
    let each = inner / n;
    let mut segs = Vec::with_capacity(kinds.len());
    let mut x = gap;
    for k in kinds {
        let x1 = x + each;
        segs.push((k, x, x1));
        x = x1 + gap;
    }
    Row { segs }
}

impl Row {
    pub fn seg_at(&self, x: u32) -> Option<Seg> {
        self.segs
            .iter()
            .find(|(_, x0, x1)| x >= *x0 && x < *x1)
            .map(|(s, _, _)| *s)
    }
}

pub fn repeatable(s: Seg) -> bool {
    matches!(s, Seg::BrightDown | Seg::BrightUp | Seg::KbdDown | Seg::KbdUp | Seg::VolDown | Seg::VolUp)
}
