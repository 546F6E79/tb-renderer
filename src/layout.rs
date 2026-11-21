//! Segment layout + hit-testing for the Touch Bar row.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    /// эластичная зона по центру: имя трека (действий нет)
    Track,
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
    pub segs: Vec<(Seg, u32, u32)>, // (seg, x0, x1); pairs have narrower gaps
}

/// Main row: [esc][moon][sun][kbd−][kbd+] ... [prev][play][next][vol−][vol+]
/// Fn row:   [F1][F2]...[F12]
pub fn build(width: u32, fn_held: bool) -> Row {
    const GAP: u32 = 10;
    const PAIR_GAP: u32 = 4;
    const BTN_ESC: u32 = 140;
    const BTN: u32 = 132;
    const BTN_MEDIA: u32 = 128;
    let kinds: Vec<Seg> = if fn_held {
        (1u8..=12).map(Seg::F).collect()
    } else {
        vec![
            Seg::Esc,
            Seg::BrightDown,
            Seg::BrightUp,
            Seg::KbdDown,
            Seg::KbdUp,
            Seg::Track,
            Seg::Prev,
            Seg::PlayPause,
            Seg::Next,
            Seg::VolDown,
            Seg::VolUp,
        ]
    };
    let n = kinds.len() as u32;
    let fixed: u32 = if fn_held {
        0
    } else {
        BTN_ESC + BTN * 6 + BTN_MEDIA * 3
    };
    let pair_gap = |a: Seg, b: Seg| {
        matches!(
            (a, b),
            (Seg::BrightDown, Seg::BrightUp)
                | (Seg::KbdDown, Seg::KbdUp)
                | (Seg::VolDown, Seg::VolUp)
        )
    };
    let gaps = if fn_held {
        GAP * (n + 1)
    } else {
        GAP * 2
            + kinds
                .windows(2)
                .map(|p| if pair_gap(p[0], p[1]) { PAIR_GAP } else { GAP })
                .sum::<u32>()
    };
    let inner = width.saturating_sub(gaps).saturating_sub(fixed);
    let each = if fn_held { inner / n } else { 0 };
    let track_w = if fn_held { 0 } else { inner };
    let mut segs = Vec::with_capacity(kinds.len());
    let mut x = GAP;
    for (i, k) in kinds.iter().copied().enumerate() {
        let w = match k {
            Seg::Track => track_w,
            _ if fn_held => each,
            Seg::Esc => BTN_ESC,
            Seg::Prev | Seg::PlayPause | Seg::Next => BTN_MEDIA,
            _ => BTN,
        };
        let x1 = x + w;
        segs.push((k, x, x1));
        if let Some(next) = kinds.get(i + 1) {
            x = x1
                + if !fn_held && pair_gap(k, *next) {
                    PAIR_GAP
                } else {
                    GAP
                };
        }
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
    matches!(
        s,
        Seg::BrightDown | Seg::BrightUp | Seg::KbdDown | Seg::KbdUp | Seg::VolDown | Seg::VolUp
    )
}
